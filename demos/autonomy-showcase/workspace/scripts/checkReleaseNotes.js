import { readFileSync } from 'node:fs';
import assert from 'node:assert/strict';

const text = readFileSync(new URL('../docs/release-notes.md', import.meta.url), 'utf8');
for (const heading of ['Autonomous Suggestions', 'Skill Routing', 'Rollback Safety']) {
  assert.match(text, new RegExp('## ' + heading), 'missing release-note section: ' + heading);
}
console.log('release notes check passed');
