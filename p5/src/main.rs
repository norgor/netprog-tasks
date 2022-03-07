#[macro_use]
extern crate rocket;

use std::{io::Write, process::Command};

use tempfile::NamedTempFile;

use rocket::fs::FileServer;

#[post("/run-code", data = "<code>")]
fn hello(code: Vec<u8>) -> Vec<u8> {
    let mut file = NamedTempFile::new().unwrap();
    file.write_all(&code).unwrap();
    let string_path = file.path().to_string_lossy();

    let output = Command::new("docker")
        .args(["run", "--mount"])
        .arg(format!("type=bind,source={},target=/main.js", string_path))
        .args(["node:17-alpine", "/main.js"])
        .output()
        .unwrap();

    if output.status.success() {
        output.stdout
    } else {
        output.stderr
    }
}

#[launch]
fn rocket() -> _ {
    rocket::build()
        .mount("/", FileServer::from("static"))
        .mount("/api/", routes![hello])
}
