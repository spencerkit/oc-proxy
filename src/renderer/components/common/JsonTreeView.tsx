import type React from "react"
import { useEffect, useMemo, useState } from "react"
import styles from "./JsonTreeView.module.css"

/** Returns true when value is an object/array container that can be expanded. */
function isContainer(value: unknown): value is Record<string, unknown> | unknown[] {
  return typeof value === "object" && value !== null
}

/** Parses unknown input into structured JSON data when possible. */
function parseStructuredValue(value: unknown): unknown | null {
  if (value === null || value === undefined) return null
  if (typeof value === "string") {
    const trimmed = value.trim()
    if (!trimmed) return null
    try {
      const parsed = JSON.parse(trimmed)
      return isContainer(parsed) ? parsed : null
    } catch {
      return null
    }
  }
  return isContainer(value) ? value : null
}

/** Converts any value into a readable text fallback for the plain-text view. */
function toText(value: unknown, emptyText: string): string {
  if (value === null || value === undefined) return emptyText
  if (typeof value === "string") return value || emptyText
  if (typeof value === "number" || typeof value === "boolean") return String(value)
  try {
    return JSON.stringify(value, null, 2)
  } catch {
    return String(value)
  }
}

/** Renders primitive JSON values with semantic styles. */
function renderPrimitive(value: unknown) {
  if (value === null) {
    return <span className={styles.valueNull}>null</span>
  }
  if (typeof value === "string") {
    return <span className={styles.valueString}>"{value}"</span>
  }
  if (typeof value === "number") {
    return <span className={styles.valueNumber}>{value}</span>
  }
  if (typeof value === "boolean") {
    return <span className={styles.valueBoolean}>{String(value)}</span>
  }
  return <span className={styles.valueFallback}>{String(value)}</span>
}

interface JsonNodeProps {
  value: unknown
  label: string | null
  path: string
  depth: number
  inArray: boolean
  expandedPaths: Set<string>
  onToggle: (path: string) => void
}

/** Renders one JSON tree node and manages expand/collapse interactions. */
function JsonNode({ value, label, path, depth, inArray, expandedPaths, onToggle }: JsonNodeProps) {
  const container = isContainer(value)
  const expanded = container ? expandedPaths.has(path) : false
  const entries = container
    ? Array.isArray(value)
      ? value.map((item, index) => [String(index), item] as const)
      : Object.entries(value)
    : []
  const summary = container
    ? Array.isArray(value)
      ? `[${entries.length} items]`
      : `{${entries.length} keys}`
    : ""

  return (
    <div>
      <div className={styles.jsonRow} style={{ paddingLeft: `${depth * 14}px` }}>
        {container ? (
          <button
            type="button"
            className={styles.toggleButton}
            onClick={() => onToggle(path)}
            aria-label={expanded ? "Collapse node" : "Expand node"}
          >
            {expanded ? "▾" : "▸"}
          </button>
        ) : (
          <span className={styles.toggleSpacer} />
        )}
        {label !== null && (
          <>
            <span className={styles.nodeKey}>{inArray ? `[${label}]` : label}</span>
            <span className={styles.nodeColon}>:</span>
          </>
        )}
        {container ? <span className={styles.nodeSummary}>{summary}</span> : renderPrimitive(value)}
      </div>
      {container && expanded && (
        <div className={styles.nodeChildren}>
          {entries.length === 0 ? (
            <div className={styles.jsonRow} style={{ paddingLeft: `${(depth + 1) * 14}px` }}>
              <span className={styles.toggleSpacer} />
              <span className={styles.emptyNode}>(empty)</span>
            </div>
          ) : (
            entries.map(([key, child]) => (
              <JsonNode
                key={`${path}.${key}`}
                value={child}
                label={key}
                path={`${path}.${key}`}
                depth={depth + 1}
                inArray={Array.isArray(value)}
                expandedPaths={expandedPaths}
                onToggle={onToggle}
              />
            ))
          )}
        </div>
      )}
    </div>
  )
}

export interface JsonTreeViewProps {
  value: unknown
  emptyText?: string
  resetKey?: string
  className?: string
}

export const JsonTreeView: React.FC<JsonTreeViewProps> = ({
  value,
  emptyText = "No data",
  resetKey,
  className,
}) => {
  const parsed = useMemo(() => parseStructuredValue(value), [value])
  const [expandedPaths, setExpandedPaths] = useState<Set<string>>(() => new Set(["$"]))

  useEffect(() => {
    void resetKey
    setExpandedPaths(new Set(["$"]))
  }, [resetKey])

  if (!parsed) {
    return <pre className={className}>{toText(value, emptyText)}</pre>
  }

  const togglePath = (path: string) => {
    setExpandedPaths(prev => {
      const next = new Set(prev)
      if (next.has(path)) {
        next.delete(path)
      } else {
        next.add(path)
      }
      return next
    })
  }

  return (
    <div className={`${styles.treeContainer} ${className ?? ""}`.trim()}>
      <JsonNode
        value={parsed}
        label={null}
        path="$"
        depth={0}
        inArray={false}
        expandedPaths={expandedPaths}
        onToggle={togglePath}
      />
    </div>
  )
}

export default JsonTreeView
