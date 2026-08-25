import { invoke } from '@tauri-apps/api/core';

export interface AppInfo {
  appName: string;
  version: string;
  platform: string;
  architecture: string;
  databaseReady: boolean;
}

export type RuntimeState =
  | 'notInstalled'
  | 'ready'
  | 'installing'
  | 'updating'
  | 'repairing'
  | 'broken'
  | 'rollbackAvailable';

export interface RuntimeStatus {
  state: RuntimeState;
  installationId: string | null;
  releaseId: string | null;
  canRollback: boolean;
  repairRequired: boolean;
}

export interface IpcErrorShape {
  code: string;
  message: string;
}

export class IpcError extends Error implements IpcErrorShape {
  readonly code: string;

  constructor(code: string, message: string) {
    super(message);
    this.name = 'IpcError';
    this.code = code;
  }
}

function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === 'object' && value !== null;
}

export function normalizeIpcError(error: unknown): IpcError {
  if (isRecord(error) && typeof error.code === 'string' && typeof error.message === 'string') {
    return new IpcError(error.code, error.message);
  }

  return new IpcError(
    'ipc_unavailable',
    'The native RetroFrontier foundation could not be reached. Start the Tauri application and try again.',
  );
}

export async function getAppInfo(): Promise<AppInfo> {
  try {
    return await invoke<AppInfo>('get_app_info');
  } catch (error: unknown) {
    throw normalizeIpcError(error);
  }
}

export async function getRuntimeStatus(): Promise<RuntimeStatus> {
  try {
    return await invoke<RuntimeStatus>('get_runtime_status');
  } catch (error: unknown) {
    throw normalizeIpcError(error);
  }
}
