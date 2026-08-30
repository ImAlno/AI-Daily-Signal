#[test]
fn display_safe_url_preserves_origin_and_path_only() {
    let original = "https://user_sentinel:password_sentinel@example.com:8443/public/path?private_query_name_sentinel=private_query_value_sentinel#private_fragment_sentinel";

    let projected = signal_core::display_safe_url(original).expect("safe URL projection");

    assert_eq!(projected, "https://example.com:8443/public/path");
    for private_material in [
        "user_sentinel",
        "password_sentinel",
        "private_query_name_sentinel",
        "private_query_value_sentinel",
        "private_fragment_sentinel",
    ] {
        assert!(!projected.contains(private_material));
    }
}
