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
  style="background: #FAF6F0; border-right: 1px solid rgba(28,25,23,0.10);"
>
  <div class="px-5 py-5" style="border-bottom: 1px solid rgba(28,25,23,0.10);">
    <a
      href="/"
      class="no-underline flex items-center gap-2.5"
      style="font-family: var(--sans-product); font-weight: 800; font-size: 17px; letter-spacing: -0.01em; color: #1C1917;"
    >
      <img src="/lq-logo.svg" alt="LegalQuants" width="28" height="28" style="display: block; flex-shrink: 0;" />
      <span>LQ Council</span>
    </a>
    <p class="mt-1 mono-label" style="font-size: 8px; letter-spacing: 0.3em; color: #78716C;">
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
            color: {active ? '#C4A052' : '#44403C'};
            background: {active ? 'rgba(196,160,82,0.10)' : 'transparent'};
            border-left: 2px solid {active ? '#C4A052' : 'transparent'};
            transition: color var(--dur-fast), background var(--dur-fast);
          "
          onmouseenter={(e) => { if (!active) { (e.currentTarget as HTMLElement).style.color = '#1C1917'; (e.currentTarget as HTMLElement).style.background = 'rgba(28,25,23,0.05)'; } }}
          onmouseleave={(e) => { if (!active) { (e.currentTarget as HTMLElement).style.color = '#44403C'; (e.currentTarget as HTMLElement).style.background = 'transparent'; } }}
        >
          <span
            style="
              font-family: var(--mono-product);
              font-size: 10px;
              letter-spacing: 0.15em;
              color: {active ? '#C4A052' : 'rgba(28,25,23,0.35)'};
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

  <div class="px-5 py-4" style="border-top: 1px solid rgba(28,25,23,0.10);">
    <p class="mono-label" style="font-size: 8px; letter-spacing: 0.25em; color: #78716C;">Session</p>
    <p
      class="mt-1"
      style="font-family: var(--sans-product); font-size: 12px; color: #78716C;"
    >
      Signed in as <span style="color: #44403C; font-weight: 500;">{role}</span>
    </p>
  </div>
</nav>
