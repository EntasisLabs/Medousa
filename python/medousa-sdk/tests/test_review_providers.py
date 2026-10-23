from medousa.types import ReviewProviderQuery, ReviewProviderResponse


def test_provider_query_preserves_opaque_external_identity():
    query = ReviewProviderQuery.model_validate({
        "provider_id": "example", "host": "git.example.test", "connection_id": "account-a",
        "request": {"operation": "get", "review": {
            "provider_id": "example", "host": "git.example.test",
            "repository": "group/subgroup/repo", "native_id": "opaque-review-id",
        }},
    })
    value = query.model_dump(mode="json")
    assert value["request"]["review"]["native_id"] == "opaque-review-id"
    assert value["connection_id"] == "account-a"


def test_provider_failures_are_typed_outcomes_even_when_transport_succeeds():
    response = ReviewProviderResponse.model_validate({
        "protocol_version": 1,
        "outcome": {"status": "error", "error": {
            "code": "stale_revision", "message": "Refresh before loading more changes",
        }},
    })
    value = response.model_dump(mode="json")
    assert value["outcome"]["error"]["code"] == "stale_revision"


def test_sign_in_public_response_has_no_device_secret_or_access_token():
    from medousa.types import ReviewProviderAuthRequest, ReviewProviderAuthResponse
    request = ReviewProviderAuthRequest.model_validate({
        "action": "begin",
        "provider_id": "example",
        "host": "example.test",
        "client_id": "public-client",
    })
    assert request.model_dump(mode="json")["client_id"] == "public-client"
    result = ReviewProviderAuthResponse.model_validate({
        "status": "pending",
        "flow_id": "opaque",
        "user_code": "ABCD-EFGH",
        "verification_uri": "https://example.test/activate",
        "expires_in": 900,
        "interval": 5,
    })
    value = result.model_dump(mode="json")
    assert "device_code" not in value and "credential" not in value


def test_publishing_uses_its_own_write_contract():
    from medousa.types import ReviewProviderMutation
    request = ReviewProviderMutation.model_validate({
        "provider_id": "example",
        "host": "example.test",
        "connection_id": "account",
        "request": {
            "operation": "publish",
            "repository": "group/subgroup/repo",
            "head": "feature",
            "base": "main",
            "title": "A change",
            "body": "Description",
            "draft": True,
            "update_existing": False,
        },
    })
    assert request.model_dump(mode="json")["request"]["repository"] == "group/subgroup/repo"
