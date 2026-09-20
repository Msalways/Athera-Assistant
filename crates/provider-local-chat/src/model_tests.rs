use crate::{default_manifest, ModelError, ModelManager};
use assistant_contracts::{conversation::InstallationStatus, Id};
use sha2::{Digest, Sha256};
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc,
};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::TcpListener,
};

#[tokio::test]
async fn verified_install_is_atomic_and_corruption_is_rejected() {
    let root = std::env::temp_dir().join(format!("companion-install-{}", Id::new_v4()));
    let body = b"GGUF fixture data";
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let server = tokio::spawn(async move {
        let (mut stream, _) = listener.accept().await.unwrap();
        let mut request = [0; 4096];
        assert!(stream.read(&mut request).await.unwrap() > 0);
        stream
            .write_all(
                format!(
                    "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                    body.len()
                )
                .as_bytes(),
            )
            .await
            .unwrap();
        stream.write_all(body).await.unwrap();
    });
    let mut manifest = default_manifest();
    manifest.url = format!("http://{address}/fixture");
    manifest.size_bytes = body.len() as u64;
    manifest.sha256 = format!("{:x}", Sha256::digest(body));
    let manager = ModelManager::with_manifest(&root, manifest);
    let installed = manager
        .install(Arc::new(AtomicBool::new(false)), Arc::new(|_| {}))
        .await
        .unwrap();
    assert_eq!(installed.status, InstallationStatus::Installed);
    assert!(manager.verify_installed().await.unwrap());
    assert!(!root.join("Qwen3-1.7B-Q4_K_M.gguf.part").exists());
    tokio::fs::write(manager.model_path(), vec![b'x'; body.len()])
        .await
        .unwrap();
    assert!(!manager.verify_installed().await.unwrap());
    manager.remove().await.unwrap();
    assert!(!manager.model_path().exists());
    server.await.unwrap();
    std::fs::remove_dir(&root).unwrap();
}

#[tokio::test]
async fn cancelling_while_waiting_for_headers_finishes_without_an_install() {
    let root = std::env::temp_dir().join(format!("companion-cancel-{}", Id::new_v4()));
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let (accepted, ready) = tokio::sync::oneshot::channel();
    let server = tokio::spawn(async move {
        let (_stream, _) = listener.accept().await.unwrap();
        accepted.send(()).unwrap();
        std::future::pending::<()>().await;
    });
    let mut manifest = default_manifest();
    manifest.url = format!("http://{address}/fixture");
    let manager = Arc::new(ModelManager::with_manifest(&root, manifest));
    let cancel = Arc::new(AtomicBool::new(false));
    let download = {
        let manager = manager.clone();
        let cancel = cancel.clone();
        tokio::spawn(async move { manager.install(cancel, Arc::new(|_| {})).await })
    };
    tokio::time::timeout(std::time::Duration::from_secs(2), ready)
        .await
        .unwrap()
        .unwrap();
    cancel.store(true, Ordering::Release);
    let result = tokio::time::timeout(std::time::Duration::from_secs(2), download)
        .await
        .unwrap()
        .unwrap();
    assert!(matches!(result, Err(ModelError::Cancelled)));
    assert_eq!(manager.installation().status, InstallationStatus::Cancelled);
    assert!(!manager.model_path().exists());
    server.abort();
    std::fs::remove_dir(&root).unwrap();
}
