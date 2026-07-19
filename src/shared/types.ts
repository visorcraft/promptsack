/** Shared domain types for PromptSack (main, core, renderer). */

export interface Prompt {
  id: string;
  title: string;
  body: string;
  notes: string;
  tags: string[];
  folderId: string | null;
  favorite: boolean;
  locked: boolean;
  /** True when body/notes are ciphertext (locked and not unlocked in session). */
  contentEncrypted?: boolean;
  createdAt: string;
  updatedAt: string;
  /** Relative markdown path under vault/prompts/ */
  mdPath?: string;
}

export interface Folder {
  id: string;
  name: string;
  locked: boolean;
  createdAt: string;
  updatedAt: string;
}

/** Saved filter (smart folder) — lives only in MongrelDB. */
export interface SmartFolder {
  id: string;
  name: string;
  filter: ListFilter;
  createdAt: string;
  updatedAt: string;
}

export interface TagInfo {
  name: string;
  count: number;
}

export type ListFilter =
  | { kind: 'all' }
  | { kind: 'favorites' }
  | { kind: 'locked' }
  | { kind: 'folder'; folderId: string }
  | { kind: 'tag'; tag: string }
  | { kind: 'smart'; smartFolderId: string };

export type SortMode = 'newest' | 'oldest' | 'title-asc' | 'title-desc';

export interface SearchHit {
  prompt: Prompt;
  score: number;
  source: 'semantic' | 'lexical' | 'hybrid';
}

export interface ExportPayload {
  version: 1;
  exportedAt: string;
  app: 'PromptSack';
  folders: Folder[];
  prompts: Prompt[];
  smartFolders?: SmartFolder[];
}

export interface CreatePromptInput {
  title: string;
  body?: string;
  notes?: string;
  tags?: string[];
  folderId?: string | null;
  favorite?: boolean;
  locked?: boolean;
}

export interface UpdatePromptInput {
  title?: string;
  body?: string;
  notes?: string;
  tags?: string[];
  folderId?: string | null;
  favorite?: boolean;
  locked?: boolean;
}

export interface CatalogSnapshot {
  prompts: Prompt[];
  folders: Folder[];
  tags: TagInfo[];
  filter: ListFilter;
  sort: SortMode;
  query: string;
}

export const EMBEDDING_DIM = 384;
export const EMBEDDING_MODEL_ID = 'Xenova/all-MiniLM-L6-v2';
export const EMBEDDING_MODEL_NAME = 'all-MiniLM-L6-v2';
