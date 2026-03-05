import { useState, useEffect, useRef, useCallback } from "react";

type AgentFilter = "all" | "claude-code" | "codex" | "gemini";

interface Props {
  onSearch: (query: string, agent: AgentFilter, category: string, showClaudeSubagents: boolean) => void;
  categories: string[];
  initialAgent?: AgentFilter;
  initialCategory?: string;
  initialShowClaudeSubagents?: boolean;
}

export function SearchBar({
  onSearch,
  categories,
  initialAgent = "all",
  initialCategory = "all",
  initialShowClaudeSubagents = false,
}: Props) {
  const [value, setValue] = useState("");
  const [agent, setAgent] = useState<AgentFilter>(initialAgent);
  const [category, setCategory] = useState(initialCategory);
  const [showClaudeSubagents, setShowClaudeSubagents] = useState(initialShowClaudeSubagents);
  const inputRef = useRef<HTMLInputElement>(null);
  const timerRef = useRef<ReturnType<typeof setTimeout>>();

  useEffect(() => {
    setAgent(initialAgent);
  }, [initialAgent]);

  useEffect(() => {
    setCategory(initialCategory);
  }, [initialCategory]);

  useEffect(() => {
    setShowClaudeSubagents(initialShowClaudeSubagents);
  }, [initialShowClaudeSubagents]);

  // Cmd+K to focus
  useEffect(() => {
    const handler = (e: KeyboardEvent) => {
      if ((e.metaKey || e.ctrlKey) && e.key === "k") {
        e.preventDefault();
        inputRef.current?.focus();
        inputRef.current?.select();
      }
    };
    window.addEventListener("keydown", handler);
    return () => window.removeEventListener("keydown", handler);
  }, []);

  const debouncedSearch = useCallback(
    (
      q: string,
      selectedAgent: AgentFilter,
      selectedCategory: string,
      selectedShowClaudeSubagents: boolean
    ) => {
      clearTimeout(timerRef.current);
      timerRef.current = setTimeout(
        () => onSearch(q, selectedAgent, selectedCategory, selectedShowClaudeSubagents),
        300
      );
    },
    [onSearch]
  );

  useEffect(() => () => clearTimeout(timerRef.current), []);

  const handleChange = (e: React.ChangeEvent<HTMLInputElement>) => {
    const q = e.target.value;
    setValue(q);
    debouncedSearch(q, agent, category, showClaudeSubagents);
  };

  const handleAgentChange = (e: React.ChangeEvent<HTMLSelectElement>) => {
    const selectedAgent = e.target.value as AgentFilter;
    setAgent(selectedAgent);
    clearTimeout(timerRef.current);
    onSearch(value, selectedAgent, category, showClaudeSubagents);
  };

  const handleCategoryChange = (e: React.ChangeEvent<HTMLSelectElement>) => {
    const selectedCategory = e.target.value;
    setCategory(selectedCategory);
    clearTimeout(timerRef.current);
    onSearch(value, agent, selectedCategory, showClaudeSubagents);
  };

  const handleSubagentToggle = (e: React.ChangeEvent<HTMLInputElement>) => {
    const checked = e.target.checked;
    setShowClaudeSubagents(checked);
    clearTimeout(timerRef.current);
    onSearch(value, agent, category, checked);
  };

  const handleKeyDown = (e: React.KeyboardEvent) => {
    if (e.key === "Escape") {
      setValue("");
      onSearch("", agent, category, showClaudeSubagents);
      inputRef.current?.blur();
    }
  };

  return (
    <div className="search-bar">
      <div className="search-controls">
        <input
          ref={inputRef}
          type="text"
          placeholder="Search sessions... (⌘K)"
          value={value}
          onChange={handleChange}
          onKeyDown={handleKeyDown}
        />
        <select value={agent} onChange={handleAgentChange} aria-label="Filter by agent">
          <option value="all">All agents</option>
          <option value="claude-code">Claude</option>
          <option value="codex">Codex</option>
          <option value="gemini">Gemini</option>
        </select>
        <select value={category} onChange={handleCategoryChange} aria-label="Filter by category">
          <option value="all">All categories</option>
          {categories.map((item) => (
            <option key={item} value={item}>
              {item}
            </option>
          ))}
        </select>
      </div>
      <label className="subagent-toggle">
        <input
          type="checkbox"
          checked={showClaudeSubagents}
          onChange={handleSubagentToggle}
          aria-label="Show Claude subagent sessions"
        />
        Show Claude subagents
      </label>
    </div>
  );
}
