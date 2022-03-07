const input = document.querySelector("#input");
const output = document.querySelector("#output");
const run = document.querySelector("#run");

function runCode(code, data) {
    return fetch("/api/run-code", {
        method: "POST",
        body: input.value,
    }).then(resp => resp.text());
}

run.addEventListener("click", async (event) => {
    output.value = await runCode(input.value);
});