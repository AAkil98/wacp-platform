"""Tests for the WACP Python SDK type constants."""

import pytest
from wacp.types import Signal, CheckpointStatus, Confidence, Priority


class TestSignal:
    def test_all_signals_defined(self):
        """All 11 signal types are defined as constants."""
        signals = [
            Signal.READY, Signal.STARTED, Signal.BLOCKED, Signal.CHECKPOINT,
            Signal.COMPLETE, Signal.FAILED, Signal.INTEGRATE,
            Signal.ACKNOWLEDGED, Signal.ESCALATION, Signal.SUSPEND,
            Signal.MIGRATE,
        ]
        assert len(signals) == 11
        assert len(set(signals)) == 11  # all unique

    def test_to_proto_valid(self):
        assert Signal.to_proto("ready") == 1
        assert Signal.to_proto("complete") == 5
        assert Signal.to_proto("migrate") == 11

    def test_to_proto_case_insensitive(self):
        assert Signal.to_proto("READY") == 1
        assert Signal.to_proto("Ready") == 1

    def test_to_proto_invalid(self):
        with pytest.raises(ValueError, match="unknown signal"):
            Signal.to_proto("nonexistent")


class TestCheckpointStatus:
    def test_values(self):
        assert CheckpointStatus.PROVISIONAL == "provisional"
        assert CheckpointStatus.FINAL == "final"

    def test_to_proto(self):
        assert CheckpointStatus.to_proto("provisional") == 1
        assert CheckpointStatus.to_proto("final") == 2

    def test_to_proto_invalid(self):
        with pytest.raises(ValueError):
            CheckpointStatus.to_proto("unknown")


class TestConfidence:
    def test_values(self):
        assert Confidence.HIGH == "high"
        assert Confidence.MEDIUM == "medium"
        assert Confidence.LOW == "low"

    def test_to_proto(self):
        assert Confidence.to_proto("high") == 1
        assert Confidence.to_proto("low") == 3


class TestPriority:
    def test_values(self):
        assert Priority.NORMAL == "normal"
        assert Priority.URGENT == "urgent"
        assert Priority.BLOCKING == "blocking"

    def test_to_proto(self):
        assert Priority.to_proto("normal") == 1
        assert Priority.to_proto("blocking") == 3
