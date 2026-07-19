/**
 * Renderer ↔ shell API surface.
 * Implemented by ui/bridge.js (Tauri) — same shape as the old Electron preload.
 */

import type {
  CreatePromptInput,
  UpdatePromptInput,
  ListFilter,
  SortMode,
  ExportPayload,
  Prompt,
  Folder,
  TagInfo,
  SearchHit,
  SmartFolder,
} from './types.js';

export type EmbeddingProviderConfig = {
  provider: 'local-minilm' | 'openai-compatible';
  baseUrl?: string;
  model?: string;
  dimensions?: number;
};

export type PromptSackApi = {
  vault: {
    status: () => Promise<{
      open: boolean;
      path: string;
      exists: boolean;
      sessionUnlocked: boolean;
      keychainBackend?: string;
      storage?: string;
      shell?: string;
    }>;
    open: (passphrase: string) => Promise<{
      ok: boolean;
      path: string;
      autoUnlocked?: boolean;
      keychainBackend?: string;
      storage?: string;
      shell?: string;
    }>;
    close: () => Promise<{ ok: boolean }>;
  };
  session: {
    unlock: (
      password: string,
      opts?: { remember?: boolean },
    ) => Promise<{ ok: boolean; keychainBackend?: string }>;
    lock: (opts?: { forgetKeychain?: boolean }) => Promise<{ ok: boolean }>;
    status: () => Promise<{
      unlocked: boolean;
      rememberLockInKeychain?: boolean;
      keychainBackend?: string;
      hasKeychainLock?: boolean;
    }>;
  };
  prompts: {
    list: (filter: ListFilter, sort: SortMode) => Promise<Prompt[]>;
    get: (id: string) => Promise<Prompt | null>;
    create: (input: CreatePromptInput) => Promise<Prompt>;
    update: (id: string, patch: UpdatePromptInput) => Promise<Prompt>;
    delete: (id: string) => Promise<{ ok: boolean }>;
    search: (query: string) => Promise<SearchHit[]>;
    bulkMove: (ids: string[], folderId: string | null) => Promise<{ count: number }>;
    bulkDelete: (ids: string[]) => Promise<{ count: number }>;
    copyBody: (id: string) => Promise<{ ok: boolean; length: number }>;
  };
  folders: {
    list: () => Promise<Folder[]>;
    create: (name: string) => Promise<Folder>;
    rename: (id: string, name: string) => Promise<Folder>;
    setLocked: (id: string, locked: boolean, password?: string) => Promise<Folder>;
    delete: (id: string, mode: 'delete' | 'move') => Promise<{ ok: boolean }>;
    export: (id: string) => Promise<ExportPayload>;
  };
  smartFolders: {
    list: () => Promise<SmartFolder[]>;
    create: (name: string, filter: ListFilter) => Promise<SmartFolder>;
    delete: (id: string) => Promise<{ ok: boolean }>;
  };
  tags: {
    list: () => Promise<TagInfo[]>;
    rename: (oldName: string, newName: string) => Promise<{ ok: boolean }>;
    delete: (name: string) => Promise<{ ok: boolean }>;
  };
  data: {
    exportAll: () => Promise<ExportPayload>;
    exportPrompts: (ids: string[]) => Promise<ExportPayload>;
    import: (
      payload: ExportPayload,
      mode: 'merge' | 'replace',
    ) => Promise<{ folders: number; prompts: number }>;
    exportToFile: (payload: ExportPayload) => Promise<{ ok: boolean; path?: string }>;
    importFromFile: () => Promise<{
      ok: boolean;
      folders?: number;
      prompts?: number;
    }>;
  };
  settings: {
    get: () => Promise<{
      embedding: EmbeddingProviderConfig;
      rememberLockInKeychain: boolean;
      hasEmbeddingApiKey: boolean;
      keychainBackend: string;
      embedderModelId: string;
      embedderReady: boolean;
      shell?: string;
      storage?: string;
    }>;
    setEmbedding: (
      config: EmbeddingProviderConfig,
      apiKey?: string | null,
    ) => Promise<{
      ok: boolean;
      config: EmbeddingProviderConfig;
      keychainBackend: string;
    }>;
    clearEmbeddingApiKey: () => Promise<{ ok: boolean }>;
    setRememberLock: (remember: boolean) => Promise<{ ok: boolean }>;
  };
  app: {
    version: () => Promise<string>;
  };
};
