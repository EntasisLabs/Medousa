from medousa.types import TurnStreamEnvelopeV2


def test_turn_stream_v2_round_trip_and_variant_validation() -> None:
    payload = {
        "schema_version": 2,
        "turn_id": "turn-1",
        "seq": 1,
        "emitted_at_utc": "2026-08-14T00:00:00Z",
        "event": {"type": "content_append", "text": "hello"},
    }
    decoded = TurnStreamEnvelopeV2.model_validate(payload)
    assert decoded.model_dump(mode="json", exclude_none=True) == payload


def test_turn_stream_v2_rejects_impossible_variant_shape() -> None:
    payload = {
        "schema_version": 2,
        "turn_id": "turn-1",
        "seq": 1,
        "emitted_at_utc": "2026-08-14T00:00:00Z",
        "event": {"type": "content_append"},
    }
    try:
        TurnStreamEnvelopeV2.model_validate(payload)
    except ValueError:
        return
    raise AssertionError("content_append without text must be rejected")
