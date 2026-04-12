// Centralized frontend type definitions shared across API, UI, and websocket layers.

/**
 * Canvas metadata returned by the backend.
 */
export interface Canvas {
  /** Unique canvas identifier. */
  id: string
  /** Human-readable canvas name. */
  name: string
  /** Canvas width in pixels/cells. */
  width: number
  /** Canvas height in pixels/cells. */
  height: number
  /** ISO timestamp for when the canvas was created. */
  createdAt: string
}

/**
 * Pixel state exchanged between backend and frontend.
 */
export interface Pixel {
  /** X coordinate in the canvas grid. */
  x: number
  /** Y coordinate in the canvas grid. */
  y: number
  /** Pixel color in hex form (`#RRGGBB`). */
  color: string
  /** Optional session/user marker used by the current local UI state. */
  updatedBy?: string
}

/**
 * Outbound websocket message emitted when a user edits a pixel.
 */
export interface PixelUpdateEvent {
  /** X coordinate being updated. */
  x: number
  /** Y coordinate being updated. */
  y: number
  /** New color for the target pixel. */
  color: string
  /** Client timestamp in Unix milliseconds for optimistic ordering. */
  clientTimestamp: number
}

/**
 * Full canvas snapshot sent by the server on initial websocket connect.
 */
export interface CanvasStateSnapshot {
  /** Canvas metadata for the active room. */
  canvas: Canvas
  /** Current pixel list representing canvas state. */
  pixels: Pixel[]
}

/**
 * Inbound websocket event confirming an accepted pixel update.
 */
export interface PixelAccepted {
  /** X coordinate that was updated. */
  x: number
  /** Y coordinate that was updated. */
  y: number
  /** Server-authoritative resulting color. */
  color: string
  /** Server-assigned version after accepting this update. */
  serverVersion: number
  /** Session id that originated the accepted update. */
  sessionId: string
}

/**
 * Inbound websocket event instructing a client to reconcile with winner state.
 */
export interface PixelRejected {
  /** X coordinate that lost the optimistic race. */
  x: number
  /** Y coordinate that lost the optimistic race. */
  y: number
  /** Winning server-authoritative color to apply immediately. */
  winningColor: string
  /** Server version of the winning state. */
  serverVersion: number
}

/**
 * Inbound websocket event emitted when a new session joins the room.
 */
export interface SessionJoined {
  /** Session id that just joined. */
  sessionId: string
  /** Active websocket session count after join. */
  activeSessionCount: number
}

/**
 * Inbound websocket event emitted when a session leaves the room.
 */
export interface SessionLeft {
  /** Session id that just left. */
  sessionId: string
  /** Active websocket session count after disconnect. */
  activeSessionCount: number
}

/**
 * Typed inbound websocket message wrapper for all protocol events.
 */
export type WsMessage =
  | { type: 'CanvasStateSnapshot'; payload: CanvasStateSnapshot }
  | { type: 'PixelAccepted'; payload: PixelAccepted }
  | { type: 'PixelRejected'; payload: PixelRejected }
  | { type: 'SessionJoined'; payload: SessionJoined }
  | { type: 'SessionLeft'; payload: SessionLeft }

/**
 * Basic health endpoint payload from the backend.
 */
export interface HealthResponse {
  /** Backend service name. */
  service: string
  /** Health status value. */
  status: string
}

/**
 * @deprecated Legacy outbound event shape used by pre-Phase 4 frontend code.
 */
export interface CanvasEvent {
  /** Legacy event type key. */
  type: 'PIXEL_UPDATED' | 'CANVAS_CLEARED' | 'SESSION_JOINED'
  /** Legacy event payload object. */
  payload: Record<string, unknown>
  /** Legacy client-side event timestamp. */
  timestamp: number
}