import { readFileSync } from 'node:fs';
import assert from 'node:assert/strict';

const html = readFileSync(new URL('../src/dashboard.html', import.meta.url), 'utf8');
assert.match(html, /<main\b[^>]*aria-label="Operations dashboard"/, 'dashboard needs a labelled main landmark');
assert.match(html, /<section\b[^>]*aria-label="Pending orders"/, 'pending orders metric needs an accessible label');
assert.match(html, /<section\b[^>]*aria-label="Risk holds"/, 'risk holds metric needs an accessible label');
console.log('dashboard accessibility check passed');
