pub fn verify_gateway_bearer(authorization: Option<&str>, expected_token: Option<&str>) -> bool {
    verify_bearer(authorization, expected_token)
}

pub fn verify_admin_bearer(authorization: Option<&str>, expected_token: Option<&str>) -> bool {
    let Some(expected) = expected_token.filter(|value| !value.is_empty()) else {
        return false;
    };
    verify_bearer(authorization, Some(expected))
}

fn verify_bearer(authorization: Option<&str>, expected_token: Option<&str>) -> bool {
    let Some(expected) = expected_token.filter(|value| !value.is_empty()) else {
        return true;
    };

    authorization
        .and_then(|value| value.strip_prefix("Bearer "))
        .is_some_and(|token| token == expected)
}

pub fn verify_policy_bearer(authorization: Option<&str>, expected_token: Option<&str>) -> bool {
    verify_bearer(authorization, expected_token)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn gateway_auth_open_when_token_unset() {
        assert!(verify_gateway_bearer(None, None));
        assert!(verify_gateway_bearer(Some("Bearer anything"), None));
    }

    #[test]
    fn gateway_auth_requires_matching_bearer() {
        assert!(verify_gateway_bearer(
            Some("Bearer secret"),
            Some("secret")
        ));
        assert!(!verify_gateway_bearer(
            Some("Bearer wrong"),
            Some("secret")
        ));
    }

    #[test]
    fn admin_auth_requires_token() {
        assert!(!verify_admin_bearer(None, None));
        assert!(verify_admin_bearer(
            Some("Bearer admin"),
            Some("admin")
        ));
    }
}
