<script lang="ts">
	import { onMount } from 'svelte';
	import { api } from '$lib/api';

	interface LoginHistoryEntry {
		id: string;
		user_id: string;
		ip_address: string;
		user_agent: string;
		method: string;
		success: boolean;
		failure_reason: string | null;
		created_at: string;
	}

	let entries = $state<LoginHistoryEntry[]>([]);
	let loading = $state(true);
	let error   = $state('');

	onMount(async () => {
		try {
			entries = await api.getJson<LoginHistoryEntry[]>('/v1/user/login-history');
		} catch (e) {
			error = e instanceof Error ? e.message : 'Failed to load login history';
		} finally {
			loading = false;
		}
	});

	function formatDate(ts: string): string {
		return new Date(ts).toLocaleString();
	}

	function methodLabel(method: string): string {
		const labels: Record<string, string> = {
			password:    'Password',
			totp:        'TOTP',
			fido2:       'Security Key',
			backup_code: 'Backup Code',
			email_otp:   'Email OTP',
		};
		return labels[method] ?? method;
	}
</script>

<svelte:head>
	<title>Login History</title>
</svelte:head>

<div class="page">
	<h1>Login History</h1>
	<p class="subtitle">Recent authentication events for your account.</p>

	{#if loading}
		<p class="loading">Loading...</p>
	{:else if error}
		<p class="error">{error}</p>
	{:else if entries.length === 0}
		<p class="empty">No login history found.</p>
	{:else}
		<div class="table-wrap">
			<table>
				<thead>
					<tr>
						<th>Time</th>
						<th>IP Address</th>
						<th>Method</th>
						<th>Status</th>
						<th>Reason</th>
						<th>User Agent</th>
					</tr>
				</thead>
				<tbody>
					{#each entries as entry (entry.id)}
						<tr class={entry.success ? 'row-success' : 'row-failure'}>
							<td class="nowrap">{formatDate(entry.created_at)}</td>
							<td class="mono">{entry.ip_address}</td>
							<td>{methodLabel(entry.method)}</td>
							<td>
								{#if entry.success}
									<span class="badge badge-ok">Success</span>
								{:else}
									<span class="badge badge-fail">Failed</span>
								{/if}
							</td>
							<td class="reason">{entry.failure_reason ?? '—'}</td>
							<td class="ua">{entry.user_agent}</td>
						</tr>
					{/each}
				</tbody>
			</table>
		</div>
	{/if}
</div>

<style>
	.page {
		max-width: 1100px;
		margin: 0 auto;
		padding: 2rem 1rem;
	}

	h1 {
		font-size: 1.5rem;
		font-weight: 700;
		margin-bottom: 0.25rem;
	}

	.subtitle {
		color: var(--color-text-muted, #6b7280);
		margin-bottom: 1.5rem;
	}

	.loading,
	.empty {
		color: var(--color-text-muted, #6b7280);
	}

	.error {
		color: var(--color-danger, #dc2626);
	}

	.table-wrap {
		overflow-x: auto;
	}

	table {
		width: 100%;
		border-collapse: collapse;
		font-size: 0.875rem;
	}

	th {
		text-align: left;
		padding: 0.5rem 0.75rem;
		border-bottom: 2px solid var(--color-border, #e5e7eb);
		font-weight: 600;
		white-space: nowrap;
	}

	td {
		padding: 0.5rem 0.75rem;
		border-bottom: 1px solid var(--color-border, #e5e7eb);
		vertical-align: top;
	}

	.row-failure td {
		background-color: var(--color-danger-bg, #fef2f2);
	}

	.nowrap { white-space: nowrap; }

	.mono {
		font-family: monospace;
		font-size: 0.8rem;
	}

	.ua {
		max-width: 260px;
		overflow: hidden;
		text-overflow: ellipsis;
		white-space: nowrap;
		color: var(--color-text-muted, #6b7280);
		font-size: 0.8rem;
	}

	.reason {
		font-size: 0.8rem;
		color: var(--color-text-muted, #6b7280);
	}

	.badge {
		display: inline-block;
		padding: 0.15rem 0.5rem;
		border-radius: 9999px;
		font-size: 0.75rem;
		font-weight: 600;
	}

	.badge-ok {
		background: var(--color-success-bg, #dcfce7);
		color: var(--color-success, #16a34a);
	}

	.badge-fail {
		background: var(--color-danger-bg, #fee2e2);
		color: var(--color-danger, #dc2626);
	}
</style>
