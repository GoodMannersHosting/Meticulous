<script lang="ts">
	import type { Member } from '$lib/api/types';

	interface Props {
		members: Member[];
		showInherited?: boolean;
		onremove?: (principalId: string) => void;
		canManage?: boolean;
	}

	let { members, showInherited = false, onremove, canManage = false }: Props = $props();

	function roleBadgeClass(role: string): string {
		switch (role) {
			case 'admin':
				return 'bg-red-500/20 text-red-400';
			case 'developer':
				return 'bg-blue-500/20 text-blue-400';
			default:
				return 'bg-zinc-500/20 text-zinc-400';
		}
	}
</script>

<div class="rounded-lg border border-zinc-700">
	<table class="w-full text-sm">
		<thead>
			<tr class="border-b border-zinc-700 text-left text-xs text-zinc-400">
				<th class="px-4 py-2">Member</th>
				<th class="px-4 py-2">Type</th>
				<th class="px-4 py-2">Role</th>
				{#if showInherited}
					<th class="px-4 py-2">Source</th>
				{/if}
				{#if canManage}
					<th class="px-4 py-2"></th>
				{/if}
			</tr>
		</thead>
		<tbody>
			{#each members as member}
				<tr class="border-b border-zinc-800 last:border-0">
					<td class="px-4 py-2 text-zinc-200">
						{member.display_name || member.principal_id}
					</td>
					<td class="px-4 py-2 text-zinc-400 capitalize">{member.principal_type}</td>
					<td class="px-4 py-2">
						<span class="rounded-full px-2 py-0.5 text-xs font-medium {roleBadgeClass(member.role)}">
							{member.role}
						</span>
					</td>
					{#if showInherited}
						<td class="px-4 py-2 text-zinc-400">
							{member.inherited ? 'Inherited from project' : 'Direct'}
						</td>
					{/if}
					{#if canManage}
						<td class="px-4 py-2 text-right">
							{#if !member.inherited}
								<button
									class="text-xs text-red-400 hover:text-red-300"
									onclick={() => onremove?.(member.principal_id)}
								>
									Remove
								</button>
							{:else}
								<span class="text-xs text-zinc-600">—</span>
							{/if}
						</td>
					{/if}
				</tr>
			{:else}
				<tr>
					<td colspan="5" class="px-4 py-6 text-center text-zinc-500">
						No members
					</td>
				</tr>
			{/each}
		</tbody>
	</table>
</div>
