<script lang="ts" module>
	export type SelectSize = 'sm' | 'md' | 'lg';

	export interface SelectOption {
		value: string;
		label: string;
		disabled?: boolean;
	}

	export interface SelectProps {
		options: SelectOption[];
		value?: string;
		placeholder?: string;
		size?: SelectSize;
		disabled?: boolean;
		error?: string;
		name?: string;
		id?: string;
		class?: string;
		onchange?: (value: string) => void;
	}
</script>

<script lang="ts">
	import { Select } from 'bits-ui';
	import { Check, ChevronDown } from 'lucide-svelte';

	let {
		options,
		value = $bindable(''),
		placeholder = 'Select...',
		size = 'md',
		disabled = false,
		error,
		name,
		id,
		class: className = '',
		onchange
	}: SelectProps = $props();

	const selectedOption = $derived(options.find((opt) => opt.value === value));

	const sizeClasses: Record<SelectSize, string> = {
		sm: 'h-8 px-3 text-sm',
		md: 'h-10 px-3 text-sm',
		lg: 'h-12 px-4 text-base'
	};
</script>

<div class="w-full {className}">
	<Select.Root type="single" {disabled} value={value} onValueChange={(v) => { if (v) { value = v; onchange?.(v); } }}>
		<Select.Trigger
			class="
				inline-flex w-full items-center justify-between rounded-lg border bg-white
				text-secondary-900 
				transition-colors duration-150
				focus:outline-none focus:ring-2 focus:ring-offset-0
				disabled:cursor-not-allowed disabled:opacity-50
				dark:bg-secondary-900 dark:text-secondary-100
				{error
				? 'border-error-500 focus:border-error-500 focus:ring-error-500'
				: 'border-secondary-300 focus:border-primary-500 focus:ring-primary-500 dark:border-secondary-600'}
				{sizeClasses[size]}
			"
			aria-invalid={!!error}
			{id}
			{name}
		>
			<span class={selectedOption ? '' : 'text-secondary-400 dark:text-secondary-500'}>
				{selectedOption?.label || placeholder}
			</span>
			<ChevronDown class="h-4 w-4 text-secondary-400" />
		</Select.Trigger>

		<Select.Portal>
			<Select.Content
				class="z-50 min-w-[8rem] overflow-hidden rounded-lg border border-secondary-200 bg-white shadow-lg dark:border-secondary-700 dark:bg-secondary-900"
				sideOffset={4}
			>
				<Select.Viewport class="p-1">
					{#each options as option (option.value)}
						<Select.Item
							value={option.value}
							disabled={option.disabled}
							class="
								relative flex cursor-pointer select-none items-center rounded-md py-2 pl-8 pr-3 text-sm
								text-secondary-900 outline-none
								data-[disabled]:pointer-events-none data-[disabled]:opacity-50
								data-[highlighted]:bg-primary-50 data-[highlighted]:text-primary-900
								dark:text-secondary-100 dark:data-[highlighted]:bg-primary-900/20 dark:data-[highlighted]:text-primary-300
							"
						>
							{#snippet children({ selected })}
								{#if selected}
									<span class="absolute left-2">
										<Check class="h-4 w-4 text-primary-600" />
									</span>
								{/if}
								{option.label}
							{/snippet}
						</Select.Item>
					{/each}
				</Select.Viewport>
			</Select.Content>
		</Select.Portal>
	</Select.Root>

	{#if error}
		<p class="mt-1.5 text-sm text-error-600 dark:text-error-500">
			{error}
		</p>
	{/if}
</div>
