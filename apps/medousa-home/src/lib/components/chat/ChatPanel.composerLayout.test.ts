import { readFileSync } from 'node:fs';
import { describe, expect, it } from 'vitest';

// happy-dom cannot measure flex geometry. Guard containment and the cascade;
// browser geometry is checked separately with the same source styles.
const source = readFileSync(new URL('./ChatPanel.svelte', import.meta.url), 'utf8');
const receipt = source.slice(source.indexOf('{#snippet projectChanges()}'), source.indexOf('{/snippet}', source.indexOf('{#snippet projectChanges()}')));
const surface = source.slice(source.indexOf('<ChatAttachmentChips disabled='), source.indexOf('<ChatRuntimeControlRow'));
const row = surface.slice(surface.indexOf('<div class="chat-context-strip"'), surface.indexOf('<form bind:this={composerFormEl}'));
describe('composer context layout contract', () => {
  it('keeps attachments then context above the separately bordered message input', () => {
    expect(row).toContain('aria-label="Chat context"');
    expect(row.indexOf('{@render projectChanges()}')).toBeGreaterThan(0);
    expect(receipt).toContain('bind:this={projectReviewReceipt} compact');
    expect(row.indexOf('<ChatWorkDock')).toBeGreaterThan(row.indexOf('{@render projectChanges()}'));
    expect(row.indexOf('<ChatStickyBoard')).toBeGreaterThan(row.indexOf('<ChatWorkDock'));
    expect(row.trimEnd()).toMatch(/<\/div>\s*\{\/if\}$/);
    expect(surface).toContain('<form bind:this={composerFormEl} class="chat-composer-message" onsubmit={submit}>');
    expect(surface.indexOf('</form>')).toBeGreaterThan(surface.indexOf('<ChatComposerBar'));
    // Neither context nor previews may be absorbed into the input outline.
    expect(source).not.toContain('chat-composer-surface');
    expect(source).not.toContain('border: 0; background: transparent; box-shadow: none;');
    expect(surface.indexOf('<ChatAttachmentChips')).toBeLessThan(surface.indexOf('<div class="chat-context-strip"'));
    expect(surface).toContain('showAttachments={false}');
    expect(surface).toMatch(/<\/form>\s*\{#if !workshop/);
    expect(source.match(/<ChatChangeReceipt\b/g)).toHaveLength(1);
    expect(source).toContain('{#if !showInlineComposer}\n        <div class="mobile-project-context">{@render projectChanges()}</div>');
    expect(source.slice(source.indexOf('<ChatScrollChrome'), source.indexOf('</ChatScrollChrome>'))).not.toContain('<ChatChangeReceipt');
  });
  it('preserves exact-project review data and explicit review actions', () => {
    expect(receipt).toContain('undertakings.review?.work_id === chatCodeProject.workId');
    expect(receipt).toContain('onRequestRevision={requestChatCodeRevision}');
    expect(row).toContain('onOpenProjectReview={() => projectReviewReceipt?.openReview()}');
  });
  it('avoids the wrapping runtime-row cascade and aligns to the input inset', () => {
    const style = source.match(/\.chat-context-strip \{([^}]+)\}/)?.[1];
    expect(style).toContain('display: flex');
    expect(style).toContain('align-items: center');
    expect(style).toContain('justify-content: space-between');
    expect(style).toContain('padding: 0 8px');
    expect(style).toContain('margin: 0 0 4px');
    expect(style).not.toContain('flex-wrap');
    expect(row).not.toContain('class="composer-context-row"');
    const chatStyles = readFileSync(new URL('../../styles/chat.postcss', import.meta.url), 'utf8');
    expect(chatStyles.match(/\.chat-composer \{([^}]+)\}/)?.[1]).toContain('pb-1 pt-0');
    for (const component of ['ChatWorkDock', 'ChatStickyBoard']) {
      const control = readFileSync(new URL(`./${component}.svelte`, import.meta.url), 'utf8');
      expect(control).not.toContain('700px');
    }
  });
  it('keeps multiple attachments in a compact horizontal rail rather than growing rows', () => {
    const styles = readFileSync(new URL('../../styles/composer.postcss', import.meta.url), 'utf8');
    const rail = styles.match(/\.composer-attachment-chips \{([^}]+)\}/)?.[1];
    expect(rail).toContain('overflow-x-auto');
    expect(rail).toContain('flex-wrap: nowrap');
    expect(rail).toContain('max-width: calc(100% - 1rem)');
    expect(styles).toContain('.composer-attachment-chips > *');
  });
});
