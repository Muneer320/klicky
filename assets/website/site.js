// Progressive enhancement only. No analytics, storage or keyboard listeners.
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
