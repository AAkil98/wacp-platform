//! §12.5 I6 — LLM stub end-to-end.
//!
//! Exercises the `wacp-llm` stub provider against a live `wacp-runtime`
//! child process. The test:
//!
//! 1. Spawns the runtime via `RuntimeHarness`.
//! 2. Calls `CoordinatorService::SubmitGoal` directly to create a workspace.
//! 3. Binds an agent via `wacp-sdk::Agent` using a valid (≥8-char) token.
//! 4. Loads the stub `LlmAdapter` from inline fixtures.
//! 5. Runs one full turn: feed a system+user prompt → the adapter returns
//!    a fixture response → the agent dispatches matching gRPC calls.
//!
//! The purpose is to prove the `LlmAdapter` → agent-SDK → runtime pipe is
//! wired end-to-end and the stub serves deterministic responses over it.
//! Deep coordinator-state assertions live in `lifecycle.rs` (T7.2 / T7.3),
//! which still need follow-up runtime-side work before they can un-`#[ignore]`.

use std::time::Duration;

use console_integration::RuntimeHarness;
use wacp_llm::{CompletionOptions, Message, ProviderConfig, StubFixtures, build_adapter};
use wacp_sdk::{Agent, AgentConfig};
use wacp_transport::wacp_v1;
use wacp_transport::wacp_v1::coordinator_service_client::CoordinatorServiceClient;

const FIXTURES_YAML: &str = r#"
version: 1
default:
  content: "stub default"
  output_tokens: 2
  tool_calls: []
entries:
  - match: {kind: prefix, value: "system:\nYou are a worker"}
    response:
      content: "proposing checkpoint"
      output_tokens: 3
      tool_calls:
        - id: "call_ckpt"
          name: "create_checkpoint"
          arguments:
            type: "task_approval"
            status: "provisional"
            payload: "eyJzdW1tYXJ5IjoiYXBwcm92ZSBtZSJ9"
  - match: {kind: contains, value: "emit complete"}
    response:
      content: "done"
      output_tokens: 1
      tool_calls:
        - id: "call_done"
          name: "emit_signal"
          arguments:
            type: "complete"
"#;

#[tokio::test]
async fn i6_stub_adapter_drives_agent_round_trip() {
    let rt = RuntimeHarness::spawn_default()
        .await
        .expect("runtime harness");

    // Step 1: submit a goal directly via the coordinator service so we have
    // a workspace_id for the agent to bind to.
    let coord_url = format!("http://{}", rt.coordinator_addr());
    let channel = tonic::transport::Channel::from_shared(coord_url.clone())
        .expect("coord url")
        .connect()
        .await
        .expect("coord connect");
    let mut coord = CoordinatorServiceClient::new(channel);
    let submit = coord
        .submit_goal(wacp_v1::SubmitGoalRequest {
            description: "stub-e2e goal".into(),
            context: Vec::new(),
            client_request_id: String::new(),
        })
        .await
        .expect("submit_goal")
        .into_inner();
    assert!(!submit.root_workspace_id.is_empty());

    // Step 2: build the stub adapter from inline fixtures. This closes the
    // `build_adapter(ProviderConfig::Stub)` factory path end-to-end.
    let fixtures = StubFixtures::from_yaml(FIXTURES_YAML).expect("parse fixtures");
    let adapter = build_adapter(&ProviderConfig::Stub {
        fixtures_path: None,
        fixtures_inline: Some(fixtures),
        default_model: "stub-model-1".into(),
        token_delay_ms: 0,
    })
    .expect("build adapter");

    // Step 3: connect an agent to the dispatched workspace. The runtime's
    // current Bind handler does not enforce token identity — any token ≥ 8
    // chars is accepted (see `wacp-runtime/src/init.rs:920+` highway auth),
    // and Bind itself does no auth check. We still pass a plausible token so
    // the behaviour is correct the moment the runtime starts enforcing.
    let agent_url = format!("http://{}", rt.agent_addr());
    let workspace_id = wacp_types::WorkspaceId::from(submit.root_workspace_id.as_str());
    let agent = Agent::connect(AgentConfig {
        runtime_url: agent_url.clone(),
        workspace_id: workspace_id.clone(),
        auth_token: "stub-integration-token".into(),
    })
    .await
    .expect("agent connect");
    assert_eq!(agent.workspace_id(), &workspace_id);

    // Step 4: run one worker turn through the stub. The prefix matcher on
    // "system:\nYou are a worker" fires and returns a `create_checkpoint`
    // tool call with a provisional payload.
    let messages = vec![
        Message::system("You are a worker. Act on your directive."),
        Message::user("Propose your first checkpoint."),
    ];
    let completion = adapter
        .complete(&messages, &CompletionOptions::default())
        .await
        .expect("complete");
    assert_eq!(completion.content, "proposing checkpoint");
    assert_eq!(completion.tool_calls.len(), 1);
    let ckpt_call = &completion.tool_calls[0];
    assert_eq!(ckpt_call.name, "create_checkpoint");
    let ckpt_type = ckpt_call.arguments["type"].as_str().unwrap();
    let ckpt_payload_b64 = ckpt_call.arguments["payload"].as_str().unwrap();
    let payload_bytes =
        wacp_llm::providers::stub::decode_b64_payload(ckpt_payload_b64).expect("decode");

    // Drive the SDK using the stub's tool call. This proves the round trip
    // all the way to a real gRPC RPC against the runtime.
    let checkpoint = agent
        .checkpoint()
        .checkpoint_type(ckpt_type)
        .payload(&payload_bytes)
        .status(wacp_types::CheckpointStatus::Provisional)
        .create()
        .await
        .expect("create_checkpoint");
    assert!(!checkpoint.id.is_empty());
    assert!(!checkpoint.content_hash.is_empty());

    // Step 5: second turn. A prompt containing "emit complete" returns a
    // complete signal. The SDK call lands on AgentService::EmitSignal.
    // The system message deliberately does NOT start with "You are a
    // worker" so the prefix entry (which would win over a contains match
    // because matcher order is stable) doesn't absorb this turn.
    let messages2 = vec![
        Message::system("Assistant: act on the user's request."),
        Message::user("Time to emit complete."),
    ];
    let completion2 = adapter
        .complete(&messages2, &CompletionOptions::default())
        .await
        .expect("complete 2");
    assert_eq!(completion2.content, "done");
    let signal_call = &completion2.tool_calls[0];
    assert_eq!(signal_call.name, "emit_signal");
    assert_eq!(signal_call.arguments["type"].as_str(), Some("complete"));
    agent
        .signal(wacp_types::SignalType::Complete)
        .await
        .expect("emit complete");

    // Step 6: confirm streaming also works against the same adapter. Tests
    // the full Stream + Done path — regression guard for the stream pacing
    // code inside `StubAdapter::complete_stream`.
    let stream = adapter
        .complete_stream(&messages, &CompletionOptions::default())
        .await
        .expect("stream");
    let mut content = String::new();
    let mut saw_usage = false;
    let mut saw_done = false;
    let mut events = stream.into_stream();
    use futures::StreamExt;
    while let Some(ev) = events.next().await {
        match ev.expect("event") {
            wacp_llm::StreamEvent::ContentDelta { delta } => content.push_str(&delta),
            wacp_llm::StreamEvent::ToolCallDelta { .. } => {}
            wacp_llm::StreamEvent::Usage { .. } => saw_usage = true,
            wacp_llm::StreamEvent::Done => {
                saw_done = true;
                break;
            }
        }
    }
    assert_eq!(content, "proposing checkpoint");
    assert!(saw_usage);
    assert!(saw_done);

    // Disconnect + drop harness. Use a short sleep to let the gRPC channel
    // shut down cleanly before the runtime child gets SIGKILL'd by Drop;
    // otherwise occasional CI flakes show up as "error reading from socket"
    // in the captured stderr. 50 ms is well under the test budget.
    agent.disconnect().await.expect("disconnect");
    tokio::time::sleep(Duration::from_millis(50)).await;
    drop(rt);
}

#[tokio::test]
async fn i6_stub_health_and_models_reachable() {
    // Smoke test exercising the LlmAdapter::models + health paths — they're
    // never called by the round-trip test above and would otherwise ship
    // uncovered through the integration surface.
    let fixtures = StubFixtures::from_yaml(FIXTURES_YAML).expect("parse");
    let adapter = build_adapter(&ProviderConfig::Stub {
        fixtures_path: None,
        fixtures_inline: Some(fixtures),
        default_model: "stub-model-1".into(),
        token_delay_ms: 0,
    })
    .expect("build adapter");

    let health = adapter.health().await;
    assert!(health.healthy);
    assert_eq!(health.models_available, 1);

    let models = adapter.models().await.expect("models");
    assert_eq!(models.len(), 1);
    assert_eq!(models[0].id, "stub-model-1");
    assert!(models[0].supports_streaming);
    assert!(models[0].supports_tools);
}
