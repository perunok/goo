import { createMockBackend } from '../backend/mock'
import { inTauri, tauriBackend } from '../backend/tauri'
import type { GooBackend } from '../backend/types'

/** The Rust side inside the app; the in-memory mock in a plain browser (`npm run dev`). */
export const backend: GooBackend = inTauri()
  ? tauriBackend
  : createMockBackend({ latencyMs: Number(new URLSearchParams(location.search).get('mockLatency') ?? 0) })
