<script lang="ts">
  let { currentPath = '/', role = 'member' }: { currentPath: string; role: string } = $props();

  const navItems = [
    { href: '/debates', label: 'Debates', icon: '\u2694', minRole: 'member' },
    { href: '/bots', label: 'Bots', icon: '\u2699', minRole: 'member' },
    { href: '/settings', label: 'Settings', icon: '\u2630', minRole: 'admin' },
    { href: '/how-it-works', label: 'How It Works', icon: '?', minRole: null as string | null },
  ];

  function isActive(href: string): boolean {
    return currentPath.startsWith(href);
  }

  function isVisible(minRole: string | null): boolean {
    if (!minRole) return true;
    if (role === 'admin') return true;
    return role === 'member' && minRole === 'member';
  }
</script>

<nav
  class="fixed left-0 top-0 h-screen w-56 bg-[var(--surface)] border-r border-[var(--border)] flex flex-col z-50"
>
  <div class="p-4 border-b border-[var(--border)]">
    <a href="/" class="mono text-lg font-bold text-[var(--text-primary)] no-underline"
      >LQ Council</
    >
  </div>
  <div class="flex-1 py-4">
    {#each navItems as item}
      {#if isVisible(item.minRole)}
        <a
          href={item.href}
          class="flex items-center gap-3 px-4 py-2.5 text-sm no-underline transition-colors {isActive(
            item.href,
          )
            ? 'text-[var(--text-primary)] bg-[rgba(139,92,246,0.1)] border-r-2 border-[#8b5cf6]'
            : 'text-[var(--text-secondary)] hover:text-[var(--text-primary)] hover:bg-[rgba(255,255,255,0.03)]'}"
        >
          <span class="mono text-xs w-4 text-center">{item.icon}</span>
          {item.label}
        </a>
      {/if}
    {/each}
  </div>
  <div class="p-4 border-t border-[var(--border)] text-xs text-[var(--text-muted)]">
    Signed in as {role}
  </div>
</nav>
