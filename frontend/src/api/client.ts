// REST client helpers for backend endpoints used by the frontend app.
import type { Canvas, CanvasStateSnapshot, HealthResponse, Pixel } from '../types'

const API_BASE_URL = import.meta.env.VITE_API_URL ?? 'http://127.0.0.1:8080'

type ApiCanvas = {
  id: string
  name: string
  width: number
  height: number
  created_at?: string
}

type ApiPixel = {
  x: number
  y: number
  color: string
  updated_by?: string | null
}

type ApiCanvasSnapshot = {
  canvas: ApiCanvas
  pixels: ApiPixel[]
}

const toCanvas = (canvas: ApiCanvas): Canvas => ({
  id: canvas.id,
  name: canvas.name,
  width: canvas.width,
  height: canvas.height,
  createdAt: canvas.created_at ?? new Date().toISOString(),
})

const toPixel = (pixel: ApiPixel): Pixel => ({
  x: pixel.x,
  y: pixel.y,
  color: pixel.color,
  updatedBy: pixel.updated_by ?? undefined,
})

async function parseJson<T>(response: Response, context: string): Promise<T> {
  if (!response.ok) {
    throw new Error(`${context} failed with status ${response.status}`)
  }

  return response.json() as Promise<T>
}

export async function getCanvases(): Promise<Canvas[]> {
  const response = await fetch(`${API_BASE_URL}/api/canvases`)
  const canvases = await parseJson<ApiCanvas[]>(response, 'List canvases')
  return canvases.map(toCanvas)
}

export async function createCanvas(name: string, width: number, height: number): Promise<Canvas> {
  const response = await fetch(`${API_BASE_URL}/api/canvases`, {
    method: 'POST',
    headers: {
      'Content-Type': 'application/json',
    },
    body: JSON.stringify({ name, width, height }),
  })

  const canvas = await parseJson<ApiCanvas>(response, 'Create canvas')
  return toCanvas(canvas)
}

export async function getCanvasSnapshot(id: string): Promise<CanvasStateSnapshot> {
  const response = await fetch(`${API_BASE_URL}/api/canvases/${id}`)
  const snapshot = await parseJson<ApiCanvasSnapshot>(response, 'Get canvas snapshot')

  return {
    canvas: toCanvas(snapshot.canvas),
    pixels: snapshot.pixels.map(toPixel),
  }
}

export async function fetchHealth(): Promise<HealthResponse> {
  const response = await fetch(`${API_BASE_URL}/`)

  if (!response.ok) {
    throw new Error(`Health check failed with status ${response.status}`)
  }

  return response.json() as Promise<HealthResponse>
}