"""WACP — Python agent SDK.

Typed async client for writing WACP agents. Connects to the runtime via gRPC,
provides a clean API for emitting signals, creating checkpoints, sending
envelopes, and receiving commands.

Usage:
    from wacp import Agent, Signal

    async def main():
        agent = await Agent.connect(
            runtime_url="localhost:9400",
            workspace_id="ws-abc123",
            auth_token="tok-xyz",
        )
        await agent.signal(Signal.STARTED)
        cp = await agent.checkpoint(
            payload=b"work product",
            type="artifact",
            intent="implemented the feature",
        )
        await agent.signal(Signal.COMPLETE)
"""

from wacp.agent import Agent
from wacp.types import Signal, CheckpointStatus, Confidence, Priority

__version__ = "0.1.0"
__all__ = ["Agent", "Signal", "CheckpointStatus", "Confidence", "Priority"]
