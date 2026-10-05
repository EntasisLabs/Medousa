from enum import Enum

from medousa.types import CodingRuntimePreferences, ExternalPeerRuntime, PeerProposalIntent


def test_runtime_enum_remains_compatible_with_existing_sdk_callers():
    assert issubclass(ExternalPeerRuntime, Enum)
    assert ExternalPeerRuntime.codex.value == "codex"
    assert ExternalPeerRuntime.medousa.value == "medousa"


def test_native_preferences_round_trip_with_ordered_fallbacks():
    value = {"preferred": "medousa", "fallbacks": ["cursor", "codex"]}
    preferences = CodingRuntimePreferences.model_validate(value)
    assert preferences.model_dump(mode="json") == value


def test_project_intake_allows_saved_preferences_without_a_runtime_override():
    intent = PeerProposalIntent.model_validate({
        "request_key": "implement",
        "forge_work_id": "work-selected",
        "instructions": "Implement the change",
        "after_entry_seq": 0,
        "through_entry_seq": 1,
        "continue_owner": False,
    })
    assert intent.runtime is None
    assert intent.forge_work_id == "work-selected"
