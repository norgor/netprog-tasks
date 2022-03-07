const input = document.querySelector("#input");
const output = document.querySelector("#output");
const run = document.querySelector("#run");

const runCode = (code) => fetch("/api/run-code", {
    method: "POST",
    body: code,
}).then(resp => resp.text());

run.addEventListener("click", async (event) => {
    output.value = await runCode(input.value);
});