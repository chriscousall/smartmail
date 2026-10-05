export function visibleMessages(messages, accounts, scope, query = '', newestFirst = true) {
  const accountById = new Map(accounts.map(account => [account.id, account]));
  const needle = query.trim().toLocaleLowerCase();
  return messages.filter(message => {
    const account = accountById.get(message.accountId);
    if (!account) return false;
    const inScope = scope === 'all' ||
      (scope === 'starred' && message.starred) ||
      (scope === 'unread' && message.unread) ||
      (scope.startsWith('group:') && account.groupId === scope.slice(6)) ||
      (scope.startsWith('account:') && account.id === scope.slice(8));
    if (!inScope) return false;
    if (!needle) return true;
    return [message.sender, message.from, message.to, message.subject, message.snippet, message.body, account.name, account.email]
      .some(value => value.toLocaleLowerCase().includes(needle));
  }).sort((a, b) => newestFirst ? Date.parse(b.time) - Date.parse(a.time) : Date.parse(a.time) - Date.parse(b.time));
}

export function replyAccount(message, accounts) {
  return accounts.find(account => account.id === message.accountId) ?? null;
}

export function groupAccountCount(accounts, groupId) {
  return accounts.filter(account => account.groupId === groupId).length;
}
