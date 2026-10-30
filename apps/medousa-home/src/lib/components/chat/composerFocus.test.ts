import { readFileSync } from 'node:fs';
import { expect, it } from 'vitest';

it('does not paint a focus-within outline or halo around composer containers in any cascade layer', () => {
  for (const sheet of ['composer', 'chat', 'theme-semantic', 'workshop-shell']) {
    const css = readFileSync(new URL(`../../styles/${sheet}.postcss`, import.meta.url), 'utf8');
    expect(css).not.toMatch(/(?:\.composer-bar(?:-mobile)?|\.chat-composer-bar)[^{}]*:focus-within\s*\{/);
  }
  const css = readFileSync(new URL('../../styles/composer.postcss', import.meta.url), 'utf8');
  expect(css).toContain('focus-visible:'); // Individual keyboard controls retain their focus treatment.
});
