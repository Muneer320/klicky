#!/usr/bin/env python3
from __future__ import annotations

import os
import shutil
import subprocess
import tempfile
import unittest
from pathlib import Path


ROOT = Path(__file__).resolve().parent.parent


class InstallLifecycleTests(unittest.TestCase):
    def make_executable(self, path: Path, content: str) -> Path:
        path.parent.mkdir(parents=True, exist_ok=True)
        path.write_text(content)
        path.chmod(0o755)
        return path

    def make_shell_environment(self, root: Path, stop_status: int = 0) -> dict[str, str]:
        fake_bin = root / "fake-bin"
        fake_bin.mkdir(parents=True, exist_ok=True)
        event_log = root / "events.log"
        systemctl_log = root / "systemctl.log"
        self.make_executable(
            fake_bin / "systemctl",
            "#!/bin/sh\n"
            'printf "systemctl %s\\n" "$*" >> "$EVENT_LOG"\n'
            'case " $* " in *" disable klicky.service "*) exit "${SYSTEMCTL_DISABLE_STATUS:-0}";; esac\n'
            "exit 0\n",
        )
        self.make_executable(fake_bin / "sudo", "#!/bin/sh\nexit 0\n")
        self.make_executable(fake_bin / "udevadm", "#!/bin/sh\nexit 0\n")
        env = os.environ.copy()
        env.update(
            {
                "HOME": str(root / "home"),
                "XDG_CONFIG_HOME": str(root / "config"),
                "PATH": str(fake_bin) + os.pathsep + env["PATH"],
                "EVENT_LOG": str(event_log),
                "SYSTEMCTL_LOG": str(systemctl_log),
                "KLICKY_STOP_STATUS": str(stop_status),
            }
        )
        Path(env["HOME"]).mkdir(parents=True, exist_ok=True)
        return env

    def run_script(self, script: Path, env: dict[str, str], *args: str):
        return subprocess.run(
            ["bash", str(script), *args],
            env=env,
            text=True,
            capture_output=True,
            check=False,
        )

    def prepare_source_install(self, root: Path, env: dict[str, str]) -> Path:
        project = root / "project"
        (project / "scripts").mkdir(parents=True)
        shutil.copy2(ROOT / "scripts/install-linux.sh",
                     project / "scripts/install-linux.sh")
        (project / "packaging/systemd").mkdir(parents=True)
        (project / "packaging/systemd/klicky.service").write_text("[Unit]\n")
        (project / "packaging/udev").mkdir(parents=True)
        (project / "packaging/udev/70-klicky.rules").write_text("# test\n")
        (project / "sounds/eg-oreo").mkdir(parents=True)
        (project / "sounds/eg-oreo/sound.ogg").write_text("bundled sound")
        fake_bin = Path(env["PATH"].split(os.pathsep)[0])
        self.make_executable(
            fake_bin / "cargo",
            "#!/bin/sh\n"
            "mkdir -p target/release\n"
            "printf '#!/bin/sh\\nexit 0\\n' > target/release/klicky\n"
            "chmod 755 target/release/klicky\n",
        )
        return project / "scripts/install-linux.sh"

    def test_source_install_stops_old_binary_before_replacing_and_preserves_user_files(self):
        with tempfile.TemporaryDirectory(prefix="klicky-source-upgrade-") as directory:
            root = Path(directory)
            env = self.make_shell_environment(root)
            home = Path(env["HOME"])
            config_root = Path(env["XDG_CONFIG_HOME"])
            old_binary = self.make_executable(
                home / ".local/bin/klicky",
                '#!/bin/sh\n[ "$1" = stop ] || exit 90\nprintf "stop\\n" >> "$EVENT_LOG"\n',
            )
            old_contents = old_binary.read_text()
            sounds = config_root / "klicky/sounds/eg-oreo"
            sounds.mkdir(parents=True)
            (sounds / "sound.ogg").write_text("user sound pack")
            (config_root / "klicky/klicky.pid").write_text("stale marker")
            (config_root / "klicky/klicky.sock").write_text("stale marker")
            installer = self.prepare_source_install(root, env)
            env["KLICKY_SKIP_UDEV"] = "1"

            result = self.run_script(installer, env)

            self.assertEqual(result.returncode, 0, result.stderr)
            self.assertNotEqual(old_contents, old_binary.read_text())
            self.assertEqual(old_binary.read_text(), "#!/bin/sh\nexit 0\n")
            self.assertEqual(
                (sounds / "sound.ogg").read_text(), "user sound pack")
            self.assertTrue((config_root / "klicky/klicky.pid").exists())
            self.assertTrue((config_root / "klicky/klicky.sock").exists())
            events = Path(env["EVENT_LOG"]).read_text().splitlines()
            self.assertLess(events.index("stop"), events.index(
                "systemctl --user daemon-reload"))

    def test_source_install_aborts_before_replacing_binary_when_stop_fails(self):
        with tempfile.TemporaryDirectory(prefix="klicky-source-upgrade-fail-") as directory:
            root = Path(directory)
            env = self.make_shell_environment(root, stop_status=17)
            home = Path(env["HOME"])
            old_binary = self.make_executable(
                home / ".local/bin/klicky",
                '#!/bin/sh\n[ "$1" = stop ] || exit 90\nexit 17\n',
            )
            original = old_binary.read_text()
            installer = self.prepare_source_install(root, env)
            env["KLICKY_SKIP_UDEV"] = "1"

            result = self.run_script(installer, env)

            self.assertNotEqual(result.returncode, 0)
            self.assertEqual(old_binary.read_text(), original)
            self.assertFalse(
                (Path(env["HOME"]) / ".local/bin/klicky").read_text().startswith("#!/bin/sh\nexit 0"))

    def prepare_uninstall(self, root: Path, env: dict[str, str]) -> tuple[Path, Path, Path]:
        home = Path(env["HOME"])
        config_root = Path(env["XDG_CONFIG_HOME"])
        binary = self.make_executable(
            home / ".local/bin/klicky",
            '#!/bin/sh\n[ "$1" = stop ] || exit 90\nprintf "stop\\n" >> "$EVENT_LOG"\nexit "${KLICKY_STOP_STATUS:-0}"\n',
        )
        unit = config_root / "systemd/user/klicky.service"
        unit.parent.mkdir(parents=True)
        unit.write_text("[Unit]\n")
        user_config = config_root / "klicky/config.toml"
        user_config.parent.mkdir(parents=True)
        user_config.write_text('sound_pack = "custom"\nvolume = 0.4\n')
        custom_sounds = user_config.parent / "sounds/custom/sound.ogg"
        custom_sounds.parent.mkdir(parents=True)
        custom_sounds.write_text("keep")
        return ROOT / "scripts/uninstall-linux.sh", binary, unit

    def test_uninstall_keeps_all_files_when_stop_or_disable_fails(self):
        for failure in ("stop", "disable"):
            with self.subTest(failure=failure), tempfile.TemporaryDirectory() as directory:
                root = Path(directory)
                stop_status = 17 if failure == "stop" else 0
                env = self.make_shell_environment(root, stop_status)
                script, binary, unit = self.prepare_uninstall(root, env)
                if failure == "disable":
                    env["SYSTEMCTL_DISABLE_STATUS"] = "9"

                result = self.run_script(script, env)

                self.assertNotEqual(result.returncode, 0)
                self.assertTrue(binary.exists())
                self.assertTrue(unit.exists())
                self.assertTrue(
                    (Path(env["XDG_CONFIG_HOME"]) / "klicky/config.toml").exists())

    def test_uninstall_is_idempotent_and_preserves_user_data_unless_purged(self):
        with tempfile.TemporaryDirectory(prefix="klicky-uninstall-") as directory:
            root = Path(directory)
            env = self.make_shell_environment(root)
            script, binary, unit = self.prepare_uninstall(root, env)
            config_file = Path(env["XDG_CONFIG_HOME"]) / "klicky/config.toml"
            sound_file = config_file.parent / "sounds/custom/sound.ogg"

            first = self.run_script(script, env)
            self.assertEqual(first.returncode, 0, first.stderr)
            self.assertFalse(binary.exists())
            self.assertFalse(unit.exists())
            self.assertTrue(config_file.exists())
            self.assertEqual(sound_file.read_text(), "keep")

            second = self.run_script(script, env)
            self.assertEqual(second.returncode, 0, second.stderr)
            self.assertTrue(config_file.exists())

            self.make_executable(
                binary,
                '#!/bin/sh\n[ "$1" = stop ] || exit 90\nprintf "stop\\n" >> "$EVENT_LOG"\n',
            )
            unit.parent.mkdir(parents=True, exist_ok=True)
            unit.write_text("[Unit]\n")
            purge = self.run_script(script, env, "--purge")
            self.assertEqual(purge.returncode, 0, purge.stderr)
            self.assertFalse(config_file.parent.exists())

    def test_debian_and_arch_guards_block_active_packaged_binary(self):
        with tempfile.TemporaryDirectory(prefix="klicky-package-guard-") as directory:
            root = Path(directory)
            proc = root / "proc"
            process = proc / "1234"
            process.mkdir(parents=True)
            (process / "exe").symlink_to("/usr/bin/klicky")

            preinst = ROOT / "packaging/debian/preinst"
            result = subprocess.run(
                ["sh", str(preinst), "check-active", str(proc)],
                text=True,
                capture_output=True,
                check=False,
            )
            self.assertNotEqual(result.returncode, 0)
            self.assertIn("PID 1234", result.stderr)

            dpkg_root = root / "dpkg-root"
            checker = dpkg_root / "var/lib/dpkg/info/klicky.preinst"
            checker.parent.mkdir(parents=True)
            shutil.copy2(preinst, checker)
            checker.chmod(0o755)
            proc_root = dpkg_root / "proc"
            (proc_root / "1234").mkdir(parents=True)
            (proc_root / "1234/exe").symlink_to("/usr/bin/klicky")
            prerm = ROOT / "packaging/debian/prerm"
            env = os.environ.copy()
            env["DPKG_ROOT"] = str(dpkg_root)
            result = subprocess.run(
                ["sh", str(prerm), "remove"],
                env=env,
                text=True,
                capture_output=True,
                check=False,
            )
            self.assertNotEqual(result.returncode, 0)
            self.assertIn("PID 1234", result.stderr)

            arch = ROOT / "packaging/arch/klicky.install"
            result = subprocess.run(
                ["bash", "-c", '. "$1"; check_packaged_daemon_stopped "$2"',
                    "test", str(arch), str(proc)],
                text=True,
                capture_output=True,
                check=False,
            )
            self.assertNotEqual(result.returncode, 0)

            (process / "exe").unlink()
            result = subprocess.run(
                ["sh", str(preinst), "check-active", str(proc)],
                text=True,
                capture_output=True,
                check=False,
            )
            self.assertEqual(result.returncode, 0, result.stderr)


if __name__ == "__main__":
    unittest.main()
