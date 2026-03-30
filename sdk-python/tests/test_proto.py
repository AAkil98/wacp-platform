"""Tests for proto type accessibility and construction (Phase T1.3)."""

import wacp.proto.v1 as pb


def test_bind_request_construction():
    req = pb.BindRequest(
        workspace_id="ws-1",
        auth_token="tok-abc",
        client_request_id="req-1",
    )
    assert req.workspace_id == "ws-1"
    assert req.auth_token == "tok-abc"
    assert req.client_request_id == "req-1"


def test_emit_signal_request_construction():
    req = pb.EmitSignalRequest(
        type=pb.SignalType.SIGNAL_TYPE_READY,
        reason="starting work",
        context=b"context data",
        client_request_id="req-2",
    )
    assert req.type == pb.SignalType.SIGNAL_TYPE_READY
    assert req.reason == "starting work"
    assert req.context == b"context data"


def test_create_checkpoint_request_construction():
    req = pb.CreateCheckpointRequest(
        type="artifact",
        payload=b"output data",
        intent="final deliverable",
        status=pb.CheckpointStatus.CHECKPOINT_STATUS_FINAL,
        confidence=pb.Confidence.CONFIDENCE_HIGH,
        resource_usage=pb.ResourceUsage(tokens=500, wall_time_ms=1000),
        client_request_id="req-3",
    )
    assert req.type == "artifact"
    assert req.payload == b"output data"
    assert req.intent == "final deliverable"
    assert req.resource_usage.tokens == 500
    assert req.resource_usage.wall_time_ms == 1000


def test_proto_enums_accessible():
    """All protocol enums are importable."""
    enums = [
        pb.SignalType, pb.WorkspaceState, pb.TaskStatus,
        pb.CheckpointStatus, pb.Confidence, pb.EnvelopePriority,
        pb.EnvelopeOrigin, pb.ErrorCategory, pb.GateType, pb.GateDecision,
    ]
    for enum_cls in enums:
        assert isinstance(enum_cls, type)


def test_proto_enum_signal_values():
    """All 11 signal type values are sequential 1–11."""
    expected = [
        pb.SignalType.SIGNAL_TYPE_READY,
        pb.SignalType.SIGNAL_TYPE_STARTED,
        pb.SignalType.SIGNAL_TYPE_BLOCKED,
        pb.SignalType.SIGNAL_TYPE_CHECKPOINT,
        pb.SignalType.SIGNAL_TYPE_COMPLETE,
        pb.SignalType.SIGNAL_TYPE_FAILED,
        pb.SignalType.SIGNAL_TYPE_INTEGRATE,
        pb.SignalType.SIGNAL_TYPE_ACKNOWLEDGED,
        pb.SignalType.SIGNAL_TYPE_ESCALATION,
        pb.SignalType.SIGNAL_TYPE_SUSPEND,
        pb.SignalType.SIGNAL_TYPE_MIGRATE,
    ]
    for i, val in enumerate(expected, start=1):
        assert val == i, f"signal type at index {i} should be {i}, got {val}"
