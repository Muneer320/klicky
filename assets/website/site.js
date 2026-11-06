// Progressive enhancement only. No analytics or storage; keyboard handling stays on tabs.
const platform = navigator.userAgentData?.platform || navigator.platform;
const unsupportedOS =
  /Android|iPhone|iPad|iPod|CrOS/i.test(navigator.userAgent) ||
  (/Mac/i.test(platform) && navigator.maxTouchPoints > 1);
const detectedOS = unsupportedOS
  ? null
  : /Win/i.test(platform)
    ? "windows"
    : /Mac/i.test(platform)
      ? "macos"
      : /Linux/i.test(platform)
        ? "linux"
        : null;

for (const group of document.querySelectorAll("[data-tabs]")) {
  const panels = [...group.children].filter((child) => child.dataset.tabLabel);
  if (!panels.length) continue;
  const list = document.createElement("div");
  list.className = "install-tabs";
  list.setAttribute("role", "tablist");
  list.setAttribute("aria-label", group.dataset.tabs);
  const buttons = panels.map((panel) => {
    const button = document.createElement("button");
    button.type = "button";
    button.id = `${panel.id}-tab`;
    button.textContent = panel.dataset.tabLabel;
    button.setAttribute("role", "tab");
    button.setAttribute("aria-controls", panel.id);
    panel.setAttribute("role", "tabpanel");
    panel.setAttribute("aria-labelledby", button.id);
    panel.tabIndex = 0;
    list.append(button);
    return button;
  });
  const select = (index) => {
    panels.forEach((panel, i) => {
      panel.hidden = i !== index;
      buttons[i].setAttribute("aria-selected", String(i === index));
      buttons[i].tabIndex = i === index ? 0 : -1;
    });
  };
  buttons.forEach((button, index) => {
    button.addEventListener("click", () => select(index));
    button.addEventListener("keydown", (event) => {
      let next;
      if (event.key === "ArrowRight") next = (index + 1) % buttons.length;
      else if (event.key === "ArrowLeft")
        next = (index - 1 + buttons.length) % buttons.length;
      else if (event.key === "Home") next = 0;
      else if (event.key === "End") next = buttons.length - 1;
      else return;
      event.preventDefault();
      select(next);
      buttons[next].focus();
    });
  });
  group.prepend(list);
  group.classList.add("tabs-ready");
  const initial =
    group.id === "install-platforms" && detectedOS
      ? panels.findIndex((panel) => panel.id === `os-${detectedOS}`)
      : 0;
  select(Math.max(0, initial));
  if (group.id === "install-platforms" && detectedOS) {
    document.getElementById("os-hint").textContent =
      `${panels[initial].dataset.tabLabel} selected for this browser. You can choose another system.`;
  }
}

const copyStatus = document.getElementById("copy-status");
for (const button of document.querySelectorAll("[data-copy]")) {
  button.hidden = false;
  button.addEventListener("click", async () => {
    const code = document.getElementById(button.dataset.copy);
    if (!code) return;
    try {
      await navigator.clipboard.writeText(code.textContent.trim());
      button.textContent = "Copied";
      if (copyStatus) copyStatus.textContent = "Command copied.";
    } catch {
      const selection = window.getSelection();
      const range = document.createRange();
      range.selectNodeContents(code);
      selection.removeAllRanges();
      selection.addRange(range);
      button.textContent = "Select & copy";
      if (copyStatus)
        copyStatus.textContent =
          "Command selected. Use your browser's Copy command.";
    }
    window.setTimeout(() => {
      button.textContent = "Copy";
    }, 2200);
  });
}
const previews = [...document.querySelectorAll("audio")];
const soundRoom = document.getElementById("sounds");
const reflectPlayback = () =>
  soundRoom?.classList.toggle(
    "is-playing",
    previews.some((audio) => !audio.paused && !audio.ended),
  );
for (const audio of previews) {
  audio.addEventListener("play", () => {
    for (const other of document.querySelectorAll("audio")) {
      if (other !== audio) other.pause();
    }
    reflectPlayback();
  });
  audio.addEventListener("pause", reflectPlayback);
  audio.addEventListener("ended", reflectPlayback);
}
