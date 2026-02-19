const idEl = document.getElementById("ext-id");
const copiedEl = document.getElementById("copied");
idEl.textContent = chrome.runtime.id;
idEl.addEventListener("click", async () => {
  await navigator.clipboard.writeText(chrome.runtime.id);
  copiedEl.style.display = "block";
  setTimeout(() => { copiedEl.style.display = "none"; }, 2000);
});
