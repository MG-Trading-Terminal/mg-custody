<script lang="ts">
	import { locale, setLocale, LOCALES } from '$lib/i18n';
	import type { Locale } from '$lib/i18n';

	let { children } = $props();

	let sidebarOpen = $state(false);

	// Nav items use translation keys — labels are re-derived whenever $locale changes.
	const userItemDefs = [
		{ href: '/',             key: 'nav.portfolio', icon: '$' },
		{ href: '/deposit',      key: 'nav.deposit',   icon: '+' },
		{ href: '/withdraw',     key: 'nav.withdraw',  icon: '-' },
		{ href: '/transactions', key: 'nav.transactions', icon: 'T' },
		{ href: '/approvals',    key: 'nav.approvals', icon: 'A' },
		{ href: '/profile',      key: 'nav.profile',   icon: 'Me' },
	];

	const adminItemDefs = [
		{ href: '/wallets',    key: 'nav.wallets',    icon: 'W' },
		{ href: '/policies',   key: 'nav.policies',   icon: 'P' },
		{ href: '/users',      key: 'nav.users',       icon: 'U' },
		{ href: '/addresses',  key: 'nav.addresses',  icon: '@' },
		{ href: '/audit',      key: 'nav.audit',       icon: 'L' },
		{ href: '/webhooks',   key: 'nav.webhooks',   icon: 'W' },
		{ href: '/derivation', key: 'nav.derivation', icon: 'D' },
		{ href: '/emergency',  key: 'nav.emergency',  icon: '!' },
		{ href: '/settings',   key: 'nav.settings',   icon: 'S' },
	];

	function closeSidebar() { sidebarOpen = false; }

	function onLocaleChange(e: Event) {
		setLocale((e.target as HTMLSelectElement).value as Locale);
	}

	// Derive translated label from the current locale store value.
	// $locale triggers re-evaluation when locale changes.
	import { t } from '$lib/i18n';
</script>

<svelte:head>
	<title>MG Custody</title>
</svelte:head>

<!-- Mobile overlay backdrop -->
{#if sidebarOpen}
	<div
		class="mobile-backdrop"
		role="presentation"
		onclick={closeSidebar}
		onkeydown={(e) => e.key === 'Escape' && closeSidebar()}
	></div>
{/if}

<div class="app">
	<nav class="sidebar" class:open={sidebarOpen}>
		<div class="logo">
			<h2>MG Custody</h2>
		</div>
		<ul>
			{#each userItemDefs as item}
				<li>
					<a href={item.href} onclick={closeSidebar}>
						<span class="icon">{item.icon}</span>
						<!-- Re-evaluates when $locale changes -->
						{$locale, t(item.key)}
					</a>
				</li>
			{/each}
		</ul>

		<div class="section-sep">
			<span>{$locale, t('nav.admin')}</span>
		</div>

		<ul>
			{#each adminItemDefs as item}
				<li>
					<a href={item.href} onclick={closeSidebar}>
						<span class="icon">{item.icon}</span>
						{$locale, t(item.key)}
					</a>
				</li>
			{/each}
		</ul>

		<!-- Language picker in sidebar footer -->
		<div class="lang-picker">
			<label class="lang-label" for="lang-select">
				&#127760;
			</label>
			<select
				id="lang-select"
				class="lang-select"
				value={$locale}
				onchange={onLocaleChange}
			>
				{#each LOCALES as loc}
					<option value={loc.code}>{loc.label}</option>
				{/each}
			</select>
		</div>
	</nav>

	<div class="content-wrap">
		<header class="mobile-header">
			<button
				class="hamburger"
				aria-label="Toggle navigation"
				aria-expanded={sidebarOpen}
				onclick={() => { sidebarOpen = !sidebarOpen; }}
			>
				<span></span>
				<span></span>
				<span></span>
			</button>
			<span class="mobile-title">MG Custody</span>
		</header>
		<main class="content">
			{@render children()}
		</main>
	</div>
</div>

<style>
	:global(body) {
		margin: 0;
		font-family: -apple-system, BlinkMacSystemFont, 'Segoe UI', sans-serif;
		background: var(--bg, #0f1117);
		color: var(--fg, #e1e4e8);
	}

	.app {
		display: flex;
		min-height: 100vh;
	}

	.sidebar {
		width: 220px;
		background: var(--sidebar-bg, #161b22);
		border-right: 1px solid var(--border, #30363d);
		padding: 1rem 0;
		flex-shrink: 0;
		display: flex;
		flex-direction: column;
	}

	.logo {
		padding: 0 1rem 1rem;
		border-bottom: 1px solid var(--border, #30363d);
		margin-bottom: 0.5rem;
	}

	.logo h2 {
		margin: 0;
		font-size: 1.1rem;
		color: var(--accent, #58a6ff);
	}

	ul {
		list-style: none;
		padding: 0;
		margin: 0;
	}

	li a {
		display: flex;
		align-items: center;
		gap: 0.75rem;
		padding: 0.6rem 1rem;
		color: var(--fg-muted, #8b949e);
		text-decoration: none;
		font-size: 0.9rem;
		transition: background 0.15s, color 0.15s;
	}

	li a:hover {
		background: var(--hover-bg, #1c2128);
		color: var(--fg, #e1e4e8);
	}

	.icon {
		width: 1.4rem;
		text-align: center;
		font-weight: bold;
		font-size: 0.78rem;
		color: var(--accent, #58a6ff);
	}

	.section-sep {
		display: flex;
		align-items: center;
		gap: 0.5rem;
		padding: 0.9rem 1rem 0.4rem;
		font-size: 0.7rem;
		font-weight: 600;
		text-transform: uppercase;
		letter-spacing: 0.08em;
		color: var(--fg-muted, #8b949e);
		border-top: 1px solid var(--border, #30363d);
		margin-top: 0.5rem;
	}

	/* Language picker — pinned to sidebar bottom */
	.lang-picker {
		margin-top: auto;
		padding: 0.75rem 1rem;
		border-top: 1px solid var(--border, #30363d);
		display: flex;
		align-items: center;
		gap: 0.5rem;
	}

	.lang-label {
		font-size: 1rem;
		cursor: pointer;
	}

	.lang-select {
		flex: 1;
		background: var(--sidebar-bg, #161b22);
		color: var(--fg-muted, #8b949e);
		border: 1px solid var(--border, #30363d);
		border-radius: 4px;
		padding: 0.3rem 0.4rem;
		font-size: 0.8rem;
		cursor: pointer;
	}

	.lang-select:focus {
		outline: 1px solid var(--accent, #58a6ff);
		color: var(--fg, #e1e4e8);
	}

	.content-wrap {
		flex: 1;
		display: flex;
		flex-direction: column;
		min-width: 0;
	}

	.mobile-header {
		display: none;
		align-items: center;
		gap: 0.75rem;
		padding: 0.75rem 1rem;
		background: var(--sidebar-bg, #161b22);
		border-bottom: 1px solid var(--border, #30363d);
		position: sticky;
		top: 0;
		z-index: 50;
	}

	.mobile-title {
		font-size: 1rem;
		font-weight: 600;
		color: var(--accent, #58a6ff);
	}

	.hamburger {
		display: none;
		flex-direction: column;
		gap: 5px;
		background: none;
		border: none;
		cursor: pointer;
		padding: 4px;
	}

	.hamburger span {
		display: block;
		width: 22px;
		height: 2px;
		background: var(--fg, #e1e4e8);
		border-radius: 2px;
		transition: opacity 0.15s;
	}

	.content {
		flex: 1;
		padding: 1.5rem 2rem;
		overflow-y: auto;
	}

	.mobile-backdrop {
		display: none;
	}

	/* -----------------------------------------------------------------------
	   Mobile breakpoint
	----------------------------------------------------------------------- */
	@media (max-width: 768px) {
		.sidebar {
			position: fixed;
			left: -220px;
			top: 0;
			z-index: 100;
			height: 100vh;
			transition: left 0.2s ease;
			overflow-y: auto;
		}

		.sidebar.open {
			left: 0;
		}

		.mobile-header {
			display: flex;
		}

		.hamburger {
			display: flex;
		}

		.content {
			padding: 1rem;
		}

		.mobile-backdrop {
			display: block;
			position: fixed;
			inset: 0;
			background: rgba(0, 0, 0, 0.5);
			z-index: 99;
		}
	}
</style>
