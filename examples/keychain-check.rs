// Run explicitly in a macOS GUI session: cargo run --locked --release --example keychain-check
// Cocoa needs the process main thread, so this cannot run inside Rust's test harness.
#[path = "../src/credentials.rs"]
mod credentials;

use anyhow::{Result, ensure};
use credentials::{Connection, CredentialStore};
use gpui::Application;

fn main() {
    Application::new().run(move |cx| {
        cx.spawn(async move |cx| {
            let key = Box::leak(
                format!("com.still.player.keychain-check-{}", std::process::id()).into_boxed_str(),
            );
            let store = CredentialStore::new(key);
            let check: Result<()> = async {
                let connection = Connection {
                    server: "https://fixture.invalid/music/".into(),
                    username: "fixture".into(),
                    password: "fixture-secret".into(),
                };
                cx.update(|cx| store.write(&connection, cx))?.await?;
                let restored = cx.update(|cx| store.read(cx))?.await?.unwrap();
                ensure!(
                    restored.server == connection.server
                        && restored.username == connection.username
                        && restored.password == connection.password,
                    "Keychain round trip failed"
                );
                let changed = Connection {
                    username: "other".into(),
                    password: "changed".into(),
                    ..connection
                };
                cx.update(|cx| store.write(&changed, cx))?.await?;
                let restored = cx.update(|cx| store.read(cx))?.await?.unwrap();
                ensure!(
                    restored.username == changed.username && restored.password == changed.password,
                    "Keychain update failed"
                );
                Ok(())
            }
            .await;
            // Always remove this isolated fixture, including after a failed assertion.
            let cleanup = cx.update(|cx| store.delete(cx)).unwrap().await;
            let deleted = cx.update(|cx| store.read(cx)).unwrap().await;
            let outcome = check.and(cleanup).and_then(|()| {
                ensure!(deleted?.is_none(), "Keychain deletion failed");
                Ok(())
            });
            if let Err(error) = outcome {
                eprintln!("Keychain check failed: {error:#}");
                std::process::exit(1);
            }
            println!("Keychain: write, restore, update and delete passed");
            cx.update(|cx| cx.quit()).unwrap();
        })
        .detach();
    });
}
