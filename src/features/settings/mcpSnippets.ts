/** Ready-to-paste MCP client configurations for goo (see src-tauri/src/mcp.rs for what they start). */
import type { McpStatus } from '../../backend/types'

export interface Snippet {
  id: string
  label: string
  hint: string
  text: string
}

const quoteShell = (s: string) => `'${s.replace(/'/g, `'\\''`)}'`

export function mcpSnippets(s: McpStatus): Snippet[] {
  const stdio = { command: s.command, args: s.args }
  const url = s.url ?? `http://127.0.0.1:${s.port}/mcp`
  return [
    {
      id: 'claude-code',
      label: 'Claude Code',
      hint: 'Run in a terminal (or use Connect above).',
      text: `claude mcp add-json --scope user goo ${quoteShell(JSON.stringify({ type: 'stdio', ...stdio }))}`,
    },
    {
      id: 'json',
      label: 'Claude Desktop, Cursor, Windsurf',
      hint: 'Add to the "mcpServers" section of the app\u2019s MCP config file.',
      text: JSON.stringify({ mcpServers: { goo: stdio } }, null, 2),
    },
    {
      id: 'vscode',
      label: 'VS Code',
      hint: 'Add to mcp.json (Command Palette: "MCP: Open User Configuration").',
      text: JSON.stringify({ servers: { goo: { type: 'stdio', ...stdio } } }, null, 2),
    },
    {
      id: 'http',
      label: 'HTTP (advanced)',
      hint: 'For clients that speak Streamable HTTP. Only works while goo is open; contains your access token.',
      text: JSON.stringify({ mcpServers: { goo: { type: 'http', url, headers: { Authorization: `Bearer ${s.token}` } } } }, null, 2),
    },
  ]
}
