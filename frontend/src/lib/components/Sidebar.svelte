<script lang="ts">
  let { currentPath, role }: { currentPath: string; role: string } = $props();

  type NavItem = {
    href: string;
    label: string;
    n: string;
    minRole: 'admin' | 'member' | null;
  };

  const navItems: NavItem[] = [
    { href: '/debates',      label: 'Debates',      n: '01', minRole: 'member' },
    { href: '/bots',         label: 'Bots',         n: '02', minRole: 'member' },
    { href: '/admins',       label: 'Admins',       n: '03', minRole: 'admin' },
    { href: '/settings',     label: 'Settings',     n: '04', minRole: 'admin' },
    { href: '/bots/guide',   label: 'Bot Guide',    n: '05', minRole: null },
    { href: '/security',     label: 'Security',     n: '06', minRole: null },
    { href: '/how-it-works', label: 'How it works', n: '07', minRole: null },
  ];

  function isActive(href: string): boolean {
    return currentPath.startsWith(href);
  }

  function isVisible(minRole: NavItem['minRole']): boolean {
    if (!minRole) return true;
    if (role === 'admin') return true;
    return role === 'member' && minRole === 'member';
  }
</script>

<nav
  class="fixed left-0 top-0 h-screen w-56 flex flex-col z-50"
  style="background: var(--night-raise); border-right: 1px solid var(--night-rule);"
>
  <div class="px-5 py-5" style="border-bottom: 1px solid var(--night-rule);">
    <a
      href="/"
      class="no-underline block"
      style="font-family: var(--sans-product); font-weight: 800; font-size: 17px; letter-spacing: -0.01em; color: var(--glow-txt);"
    >
      LQ Council
    </a>
    <p class="mt-1 mono-label" style="font-size: 8px; letter-spacing: 0.3em;">
      Agentic Playground
    </p>
  </div>

  <div class="flex-1 py-3">
    {#each navItems as item}
      {#if isVisible(item.minRole)}
        {@const active = isActive(item.href)}
        <a
          href={item.href}
          class="flex items-center gap-3 px-5 py-2.5 no-underline"
          style="
            font-family: var(--sans-product);
            font-size: 13px;
            font-weight: {active ? 600 : 500};
            color: {active ? 'var(--glow-txt)' : 'var(--glow-mute)'};
            background: {active ? 'rgba(99,102,241,0.08)' : 'transparent'};
            border-right: 2px solid {active ? 'var(--indigo-500)' : 'transparent'};
            transition: color var(--dur-fast), background var(--dur-fast);
          "
          onmouseenter={(e) => { if (!active) (e.currentTarget as HTMLElement).style.color = 'var(--glow-txt)'; }}
          onmouseleave={(e) => { if (!active) (e.currentTarget as HTMLElement).style.color = 'var(--glow-mute)'; }}
        >
          <span
            style="
              font-family: var(--mono-product);
              font-size: 10px;
              letter-spacing: 0.15em;
              color: {active ? 'var(--copper)' : 'var(--glow-faint)'};
              min-width: 18px;
            "
          >
            {item.n}
          </span>
          <span>{item.label}</span>
        </a>
      {/if}
    {/each}
  </div>

  <div class="px-5 py-4" style="border-top: 1px solid var(--night-rule);">
    <p class="mono-label" style="font-size: 8px; letter-spacing: 0.25em;">Session</p>
    <p
      class="mt-1"
      style="font-family: var(--sans-product); font-size: 12px; color: var(--glow-dim);"
    >
      Signed in as <span style="color: var(--glow-txt); font-weight: 500;">{role}</span>
    </p>
  </div>
</nav>
