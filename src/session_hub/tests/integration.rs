use session_hub::HubState;
use session_common::{Snapshot, ActiveSession};

fn make_session(session_id: &str, provider: &str, last_activity: i64, last_message: Option<&str>) -> ActiveSession {
    ActiveSession {
        session_id: session_id.to_string(),
        provider: provider.to_string(),
        agent_id: None,
        agent_type: "main".to_string(),
        model: "opus".to_string(),
        status: "active".to_string(),
        last_activity,
        project: None,
        last_message: last_message.map(String::from),
        last_tool: None,
        last_tool_input: None,
        parent_session_id: None,
    }
}

fn make_snapshot(collector_id: &str, timestamp: i64, fingerprint: &str, sessions: Vec<ActiveSession>) -> Snapshot {
    Snapshot {
        collector_id: collector_id.to_string(),
        timestamp,
        fingerprint: fingerprint.to_string(),
        sessions,
    }
}

#[tokio::test]
async fn test_snapshot_merge() {
    let state = HubState::new();
    
    let snapshot1 = make_snapshot(
        "machine-1", 1000, "abc",
        vec![make_session("s1", "claude", 1000, Some("first"))],
    );
    
    state.apply_snapshot(snapshot1).await;
    let sessions = state.get_all_sessions().await;
    assert_eq!(sessions.len(), 1);
    assert_eq!(sessions[0].session_id, "s1");
    assert_eq!(sessions[0].last_message.as_deref(), Some("first"));
    
    // Snapshot from another collector with same session (newer activity wins)
    let snapshot2 = make_snapshot(
        "machine-2", 2000, "def",
        vec![make_session("s1", "claude", 2000, Some("newer"))],
    );
    
    state.apply_snapshot(snapshot2).await;
    let sessions = state.get_all_sessions().await;
    assert_eq!(sessions.len(), 1);
    assert_eq!(sessions[0].last_activity, 2000);
    assert_eq!(sessions[0].last_message.as_deref(), Some("newer"));
}

#[tokio::test]
async fn test_latest_wins() {
    let state = HubState::new();
    
    let s1 = make_session("s1", "claude", 1000, Some("newer"));
    state.apply_snapshot(make_snapshot("c1", 1000, "f1", vec![s1])).await;
    
    let s1_old = make_session("s1", "claude", 500, Some("older"));
    state.apply_snapshot(make_snapshot("c2", 2000, "f2", vec![s1_old])).await;
    
    let sessions = state.get_all_sessions().await;
    assert_eq!(sessions.len(), 1);
    assert_eq!(sessions[0].last_activity, 1000); // newer one wins
}

#[tokio::test]
async fn test_collector_removal() {
    let state = HubState::new();
    
    let snapshot1 = make_snapshot(
        "machine-1", 1000, "abc",
        vec![make_session("s1", "claude", 1000, None)],
    );
    state.apply_snapshot(snapshot1).await;
    
    assert_eq!(state.get_all_sessions().await.len(), 1);
    
    state.remove_collector("machine-1").await;
    
    assert_eq!(state.get_all_sessions().await.len(), 0);
}
