import { useState, useEffect } from "react";
import { getHealth, getSessionTranscript, indexSessions } from "../services/cass";
import { tagSession } from "../services/tags";
import type { Session, SessionTag } from "../types";

interface Props {
  sessions: Session[];
  tags: Record<string, SessionTag>;
  onTagUpdate: (tag: SessionTag) => void;
}

function sleep(ms: number) {
  return new Promise((resolve) => setTimeout(resolve, ms));
}

export function StatusBar({ sessions, tags, onTagUpdate }: Props) {
  const [healthy, setHealthy] = useState<boolean | null>(null);
  const [indexing, setIndexing] = useState(false);
  const [tagging, setTagging] = useState(false);
  const [tagProgress, setTagProgress] = useState<{ current: number; total: number } | null>(null);

  useEffect(() => {
    getHealth().then((h) => setHealthy(h.healthy));
    const interval = setInterval(() => {
      getHealth().then((h) => setHealthy(h.healthy));
    }, 30000);
    return () => clearInterval(interval);
  }, []);

  const handleReindex = async () => {
    setIndexing(true);
    await indexSessions();
    setIndexing(false);
    const h = await getHealth();
    setHealthy(h.healthy);
  };

  const handleTagAll = async () => {
    const untagged = sessions.filter((session) => !tags[session.id]);
    if (untagged.length === 0) return;

    setTagging(true);
    setTagProgress({ current: 0, total: untagged.length });

    for (let i = 0; i < untagged.length; i += 1) {
      const session = untagged[i];
      try {
        const messages = await getSessionTranscript(session.sourcePath);
        const created = await tagSession(session, messages);
        onTagUpdate(created);
      } catch (err) {
        console.error(`Tag all failed for ${session.id}:`, err);
      }

      setTagProgress({ current: i + 1, total: untagged.length });
      await sleep(200);
    }

    setTagging(false);
    setTagProgress(null);
  };

  const tagButtonLabel = tagProgress
    ? `Tagging ${tagProgress.current}/${tagProgress.total}...`
    : "Tag All";

  return (
    <div className="status-bar" data-testid="status-bar">
      <div className="status-indicator">
        <span className={`dot ${healthy === true ? "green" : healthy === false ? "red" : "gray"}`} />
        <span>CASS {healthy === true ? "healthy" : healthy === false ? "offline" : "checking..."}</span>
      </div>
      <div className="status-actions">
        <button className="reindex-btn" onClick={handleReindex} disabled={indexing || tagging}>
          {indexing ? "Indexing..." : "Reindex"}
        </button>
        <button
          className="reindex-btn"
          onClick={handleTagAll}
          disabled={indexing || tagging || sessions.length === 0}
        >
          {tagging ? tagButtonLabel : "Tag All"}
        </button>
      </div>
    </div>
  );
}
