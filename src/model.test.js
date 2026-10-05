import test from 'node:test';
import assert from 'node:assert/strict';
import { visibleMessages, replyAccount, groupAccountCount } from './model.js';

const accounts = [
  { id: 'personal', groupId: 'home', email: 'person@example.test', name: 'Personal' },
  { id: 'support', groupId: 'work', email: 'support@example.test', name: 'Support' },
];
const messages = [
  { id: 'one', accountId: 'personal', sender: 'A', from: 'a@example.test', to: 'person@example.test', subject: 'Hello', snippet: '', body: '', time: '2026-01-01T12:00:00Z', unread: true, starred: false },
  { id: 'two', accountId: 'support', sender: 'B', from: 'b@example.test', to: 'support@example.test', subject: 'Question', snippet: '', body: '', time: '2026-01-02T12:00:00Z', unread: false, starred: true },
];

test('a group inbox contains only messages from its accounts', () => {
  const personal = visibleMessages(messages, accounts, 'group:home');
  assert.deepEqual(personal.map(message => message.id), ['one']);
});

test('moving an account changes its group view without changing message ownership', () => {
  const moved = structuredClone(accounts);
  moved.find(account => account.id === 'support').groupId = 'home';
  assert.deepEqual(visibleMessages(messages, moved, 'group:home').map(message => message.id), ['two', 'one']);
  assert.equal(messages.find(message => message.id === 'two').accountId, 'support');
  assert.equal(groupAccountCount(moved, 'work'), 0);
});

test('reply uses the receiving account even in a combined inbox', () => {
  assert.equal(replyAccount(messages[1], accounts)?.email, 'support@example.test');
});

test('search stays within the selected account or group', () => {
  assert.deepEqual(visibleMessages(messages, accounts, 'account:personal', 'question').map(message => message.id), []);
  assert.deepEqual(visibleMessages(messages, accounts, 'group:work', 'question').map(message => message.id), ['two']);
});
