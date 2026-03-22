"""Protocol type constants — Pythonic wrappers over proto enums."""


class Signal:
    """Signal type constants (closed set of 11)."""
    READY = "ready"
    STARTED = "started"
    BLOCKED = "blocked"
    CHECKPOINT = "checkpoint"
    COMPLETE = "complete"
    FAILED = "failed"
    INTEGRATE = "integrate"
    ACKNOWLEDGED = "acknowledged"
    ESCALATION = "escalation"
    SUSPEND = "suspend"
    MIGRATE = "migrate"

    _TO_PROTO = {
        "ready": 1,
        "started": 2,
        "blocked": 3,
        "checkpoint": 4,
        "complete": 5,
        "failed": 6,
        "integrate": 7,
        "acknowledged": 8,
        "escalation": 9,
        "suspend": 10,
        "migrate": 11,
    }

    @classmethod
    def to_proto(cls, signal: str) -> int:
        """Convert a signal string to its proto enum value."""
        val = cls._TO_PROTO.get(signal.lower())
        if val is None:
            raise ValueError(f"unknown signal type: {signal}")
        return val


class CheckpointStatus:
    PROVISIONAL = "provisional"
    FINAL = "final"

    _TO_PROTO = {"provisional": 1, "final": 2}

    @classmethod
    def to_proto(cls, status: str) -> int:
        val = cls._TO_PROTO.get(status.lower())
        if val is None:
            raise ValueError(f"unknown checkpoint status: {status}")
        return val


class Confidence:
    HIGH = "high"
    MEDIUM = "medium"
    LOW = "low"

    _TO_PROTO = {"high": 1, "medium": 2, "low": 3}

    @classmethod
    def to_proto(cls, confidence: str) -> int:
        val = cls._TO_PROTO.get(confidence.lower())
        if val is None:
            raise ValueError(f"unknown confidence: {confidence}")
        return val


class Priority:
    NORMAL = "normal"
    URGENT = "urgent"
    BLOCKING = "blocking"

    _TO_PROTO = {"normal": 1, "urgent": 2, "blocking": 3}

    @classmethod
    def to_proto(cls, priority: str) -> int:
        val = cls._TO_PROTO.get(priority.lower())
        if val is None:
            raise ValueError(f"unknown priority: {priority}")
        return val
