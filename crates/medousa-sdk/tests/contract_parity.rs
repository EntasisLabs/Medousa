//! Generated operations are the SDK contract. Handwritten PARITY_ROUTES is gone.

use medousa_sdk::operations;

#[test]
fn generated_ops_are_unique_http_methods() {
    let mut seen = std::collections::HashSet::new();
    assert!(!operations::ALL.is_empty());
    for op in operations::ALL {
        assert_ne!(op.method, "SSE", "SSE is not an HTTP method: {}", op.id);
        assert!(
            matches!(op.method, "GET" | "POST" | "PUT" | "PATCH" | "DELETE" | "HEAD" | "OPTIONS"),
            "unexpected method {} for {}",
            op.method,
            op.id
        );
        assert!(op.path.starts_with('/'));
        assert!(!op.path.contains('?'));
        assert!(seen.insert((op.method, op.path)), "duplicate {} {}", op.method, op.path);
    }
}

#[test]
fn health_and_liveness_are_distinct() {
    assert_eq!(operations::LIVENESS_GET.path, "/health");
    assert_eq!(operations::HEALTH_GET.path, "/v1/health");
    assert_eq!(operations::by_id("health.get").unwrap().path, "/v1/health");
}