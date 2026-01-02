use sitewrap_portal::open_uri;

// Smoke test for API surface; it does not assert success because portals may be
// unavailable in CI. It should compile and not panic on call.
#[test]
fn open_uri_api_smoke() {
    let _ = open_uri("https://example.com");
}

#[test]
fn open_uri_rejects_invalid_uri() {
    let result = open_uri("example.com");
    assert!(result.is_err());
}
