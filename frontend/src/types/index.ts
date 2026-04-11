// Centralized frontend type definitions shared across components, hooks, and API.
export type Pixel = {
  x: number
  y: number
  color: string
  updatedBy: string
}

export type CanvasEvent = {
  type: 'PIXEL_UPDATED' | 'CANVAS_CLEARED' | 'SESSION_JOINED'
  payload: Record<string, unknown>
  timestamp: number
}

export type HealthResponse = {
  service: string
  status: string
}