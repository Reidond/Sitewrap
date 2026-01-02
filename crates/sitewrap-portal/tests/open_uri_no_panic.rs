use sitewrap_portal::open_uri;

#[test]
#[ignore = "opens browser when portals are available - run with --ignored in CI only"]
fn open_uri_api_smoke() {
    let _ = open_uri("https://example.com");
}

#[test]
fn open_uri_rejects_invalid_uri() {
    let result = open_uri("example.com");
    assert!(result.is_err());
}
