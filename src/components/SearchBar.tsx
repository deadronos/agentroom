// src/components/SearchBar.tsx
import { useState, useEffect, useRef, useCallback } from "react";

interface Props {
  onSearch: (query: string, agent: string, category: string, showSubagents: boolean) => void;
  categories: string[];
  availableProviders: string[];        // NEW — from hub, dynamically populated
  initialAgent?: string;
  initialCategory?: string;
  initialShowSubagents?: boolean;
}

export function SearchBar({
  onSearch,
  categories,
  availableProviders,
  initialAgent = 'all',
  initialCategory = 'all',
  initialShowSubagents = false,
}: Props) {
  const [value, setValue] = useState('');
  const [agent, setAgent] = useState<string>(initialAgent);
  const [category, setCategory] = useState(initialCategory);
  const [showSubagents, setShowSubagents] = useState(initialShowSubagents);
  const inputRef = useRef<HTMLInputElement>(null);
  const timerRef = useRef<ReturnType<typeof setTimeout>>();

  useEffect(() => { setAgent(initialAgent); }, [initialAgent]);
  useEffect(() => { setCategory(initialCategory); }, [initialCategory]);
  useEffect(() => { setShowSubagents(initialShowSubagents); }, [initialShowSubagents]);

  useEffect(() => {
    const handler = (e: KeyboardEvent) => {
      if ((e.metaKey || e.ctrlKey) && e.key === 'k') {
        e.preventDefault();
        inputRef.current?.focus();
        inputRef.current?.select();
      }
    };
    window.addEventListener('keydown', handler);
    return () => window.removeEventListener('keydown', handler);
  }, []);

  const debouncedSearch = useCallback(
    (q: string, selectedAgent: string, selectedCategory: string, selectedShowSubagents: boolean) => {
      clearTimeout(timerRef.current);
      timerRef.current = setTimeout(
        () => onSearch(q, selectedAgent, selectedCategory, selectedShowSubagents),
        300
      );
    },
    [onSearch]
  );

  useEffect(() => () => clearTimeout(timerRef.current), []);

  const handleChange = (e: React.ChangeEvent<HTMLInputElement>) => {
    const q = e.target.value;
    setValue(q);
    debouncedSearch(q, agent, category, showSubagents);
  };

  const handleAgentChange = (e: React.ChangeEvent<HTMLSelectElement>) => {
    const selectedAgent = e.target.value;
    setAgent(selectedAgent);
    clearTimeout(timerRef.current);
    onSearch(value, selectedAgent, category, showSubagents);
  };

  const handleCategoryChange = (e: React.ChangeEvent<HTMLSelectElement>) => {
    const selectedCategory = e.target.value;
    setCategory(selectedCategory);
    clearTimeout(timerRef.current);
    onSearch(value, agent, selectedCategory, showSubagents);
  };

  const handleSubagentToggle = (e: React.ChangeEvent<HTMLInputElement>) => {
    const checked = e.target.checked;
    setShowSubagents(checked);
    clearTimeout(timerRef.current);
    onSearch(value, agent, category, checked);
  };

  const handleKeyDown = (e: React.KeyboardEvent) => {
    if (e.key === 'Escape') {
      setValue('');
      onSearch('', agent, category, showSubagents);
      inputRef.current?.blur();
    }
  };

  return (
    <div className="search-bar" data-testid="search-bar">
      <div className="search-controls">
        <input
          ref={inputRef}
          type="text"
          data-testid="search-input"
          placeholder="Search sessions... (⌘K)"
          value={value}
          onChange={handleChange}
          onKeyDown={handleKeyDown}
        />
        <select data-testid="agent-filter" value={agent} onChange={handleAgentChange} aria-label="Filter by agent">
          <option value="all">All agents</option>
          {availableProviders.map((p) => (
            <option key={p} value={p}>{p}</option>
          ))}
        </select>
        <select data-testid="category-filter" value={category} onChange={handleCategoryChange} aria-label="Filter by category">
          <option value="all">All categories</option>
          {categories.map((item) => (
            <option key={item} value={item}>{item}</option>
          ))}
        </select>
      </div>
      <label className="subagent-toggle">
        <input
          type="checkbox"
          checked={showSubagents}
          onChange={handleSubagentToggle}
          aria-label="Show subagent sessions"
        />
        Show subagents
      </label>
    </div>
  );
}
