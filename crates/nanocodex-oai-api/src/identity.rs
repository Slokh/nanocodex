pub(crate) fn new_uuid_v7() -> uuid::Uuid {
    #[cfg(not(all(target_family = "wasm", target_os = "unknown")))]
    {
        uuid::Uuid::now_v7()
    }

    #[cfg(all(target_family = "wasm", target_os = "unknown"))]
    {
        let elapsed = web_time::SystemTime::now()
            .duration_since(web_time::UNIX_EPOCH)
            .unwrap_or_default();
        uuid::Uuid::new_v7(uuid::Timestamp::from_unix(
            uuid::NoContext,
            elapsed.as_secs(),
            elapsed.subsec_nanos(),
        ))
    }
}
