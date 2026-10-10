import { mcpSnippets } from './mcpSnippets'
import type { McpStatus } from '../../backend/types'

const status: McpStatus = {
  enabled: true,
  running: true,
  port: 7355,
  url: 'http://127.0.0.1:7355/mcp',
  error: null,
  token: 'goo_abc',
  command: "/home/o'neil/Apps/goo.AppImage",
  args: ['--mcp-stdio'],
  devBuild: false,
  claudeCode: { found: true, connected: 'no' },
}

describe('MCP snippets', () => {
  const byId = Object.fromEntries(mcpSnippets(status).map((s) => [s.id, s.text]))

  it('gives stdio configs that launch goo --mcp-stdio', () => {
    expect(JSON.parse(byId.json!)).toEqual({ mcpServers: { goo: { command: status.command, args: ['--mcp-stdio'] } } })
    expect(JSON.parse(byId.vscode!).servers.goo.type).toBe('stdio')
  })

  it('quotes the Claude Code command for the shell', () => {
    expect(byId['claude-code']).toBe(
      `claude mcp add-json --scope user goo '{"type":"stdio","command":"/home/o'\\''neil/Apps/goo.AppImage","args":["--mcp-stdio"]}'`,
    )
  })

  it('puts the token only in the HTTP config', () => {
    expect(byId.http).toContain('Bearer goo_abc')
    for (const id of ['claude-code', 'json', 'vscode']) expect(byId[id]).not.toContain('goo_abc')
  })
})
