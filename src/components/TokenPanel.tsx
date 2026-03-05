import { useState, useEffect } from "react";
import { readCodexBarSnapshot } from "../bridge";

interface ProviderEntry {
  provider: string;
  tokenUsage?: {
    sessionCostUSD: number;
    sessionTokens: number;
    last30DaysCostUSD: number;
    last30DaysTokens: number;
  };
  primary?: { usedPercent: number; windowMinutes: number; resetDescription: string };
  secondary?: { usedPercent: number; windowMinutes: number; resetDescription: string };
  dailyUsage?: Array<{ dayKey: string; costUSD: number; totalTokens: number }>;
}

interface Snapshot {
  entries: ProviderEntry[];
  enabledProviders: string[];
  generatedAt: string;
}

const PROVIDER_COLORS: Record<string, string> = {
  claude: "#e87b35",
  codex: "#3fb950",
  gemini: "#a78bfa",
};

const PROVIDER_LABELS: Record<string, string> = {
  claude: "Claude",
  codex: "Codex",
  gemini: "Gemini",
};

function formatCost(usd: number): string {
  if (usd < 0.01) return "<$0.01";
  if (usd < 1) return `$${usd.toFixed(2)}`;
  return `$${usd.toFixed(1)}`;
}

function todayCost(entry: ProviderEntry): number {
  const today = new Date().toISOString().split("T")[0];
  const todayEntry = entry.dailyUsage?.find((d) => d.dayKey === today);
  return todayEntry?.costUSD || entry.tokenUsage?.sessionCostUSD || 0;
}

export function TokenPanel() {
  const [snapshot, setSnapshot] = useState<Snapshot | null>(null);

  useEffect(() => {
    const load = async () => {
      const data = (await readCodexBarSnapshot()) as Snapshot | null;
      if (data?.entries) setSnapshot(data);
    };
    load();
    const interval = setInterval(load, 60_000);
    return () => clearInterval(interval);
  }, []);

  if (!snapshot || snapshot.entries.length === 0) return null;

  return (
    <div className="token-panel" data-testid="token-panel">
      {snapshot.entries.map((entry) => {
        const color = PROVIDER_COLORS[entry.provider] || "#888";
        const label = PROVIDER_LABELS[entry.provider] || entry.provider;
        const primaryUsed = entry.primary?.usedPercent ?? 0;
        const secondaryUsed = entry.secondary?.usedPercent ?? 0;
        const primaryLeft = Math.max(0, 100 - primaryUsed);
        const secondaryLeft = Math.max(0, 100 - secondaryUsed);

        return (
          <div key={entry.provider} className="token-provider">
            <div className="token-provider-header">
              <span className="token-provider-dot" style={{ background: color }} />
              <span className="token-provider-name">{label}</span>
              <span className="token-today-cost">{formatCost(todayCost(entry))}</span>
            </div>
            <div className="token-bars">
              <div className="token-bar-row">
                <span className="token-bar-label">5h</span>
                <div className="token-bar-track">
                  <div
                    className="token-bar-fill"
                    style={{
                      width: `${Math.min(primaryLeft, 100)}%`,
                      background: primaryLeft < 20 ? "#f85149" : color,
                    }}
                  />
                </div>
                <span className="token-bar-pct">{Math.round(primaryLeft)}%</span>
              </div>
              <div className="token-bar-row">
                <span className="token-bar-label">7d</span>
                <div className="token-bar-track">
                  <div
                    className="token-bar-fill"
                    style={{
                      width: `${Math.min(secondaryLeft, 100)}%`,
                      background: secondaryLeft < 20 ? "#f85149" : color,
                    }}
                  />
                </div>
                <span className="token-bar-pct">{Math.round(secondaryLeft)}%</span>
              </div>
            </div>
          </div>
        );
      })}
    </div>
  );
}
