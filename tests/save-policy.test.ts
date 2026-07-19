/**
 * Regression: save-policy must never emit a body/notes wipe for encrypted placeholders.
 * Drives the shipped buildPromptSavePatch helper used by the renderer.
 */
import { describe, it, expect } from 'vitest';
import { buildPromptSavePatch } from '../src/shared/save-policy.js';

describe('buildPromptSavePatch', () => {
  it('refuses save when content is encrypted and session is locked', () => {
    const result = buildPromptSavePatch({
      title: 'Secret',
      body: '',
      notes: '',
      tags: [],
      folderId: null,
      favorite: false,
      locked: true,
      contentEncrypted: true,
      sessionUnlocked: false,
    });
    expect(result.ok).toBe(false);
    if (!result.ok) {
      expect(result.needsUnlock).toBe(true);
      expect(result.reason.toLowerCase()).toMatch(/unlock/);
    }
  });

  it('refuses save when content is encrypted even after unlock (needs reload)', () => {
    const result = buildPromptSavePatch({
      title: 'Secret',
      body: '', // placeholder still in form
      notes: '',
      tags: [],
      folderId: null,
      favorite: false,
      locked: true,
      contentEncrypted: true,
      sessionUnlocked: true,
    });
    expect(result.ok).toBe(false);
    if (!result.ok) {
      expect(result.needsUnlock).toBe(false);
      expect(result.reason.toLowerCase()).toMatch(/reload|loaded/);
    }
  });

  it('emits full patch when content is loaded plaintext', () => {
    const result = buildPromptSavePatch({
      title: 'Secret',
      body: 'classified body',
      notes: 'classified notes',
      tags: ['a'],
      folderId: null,
      favorite: true,
      locked: true,
      contentEncrypted: false,
      sessionUnlocked: true,
    });
    expect(result.ok).toBe(true);
    if (result.ok) {
      expect(result.patch.body).toBe('classified body');
      expect(result.patch.notes).toBe('classified notes');
      expect(result.patch.locked).toBe(true);
    }
  });
});
