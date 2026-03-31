<script lang="ts" module>
	export type InputSize = 'sm' | 'md' | 'lg';

	export interface InputProps {
		type?: 'text' | 'email' | 'password' | 'number' | 'search' | 'tel' | 'url';
		size?: InputSize;
		value?: string;
		placeholder?: string;
		disabled?: boolean;
		readonly?: boolean;
		required?: boolean;
		name?: string;
		id?: string;
		autocomplete?: AutoFill;
		error?: string;
		class?: string;
		oninput?: (event: Event & { currentTarget: HTMLInputElement }) => void;
		onchange?: (event: Event & { currentTarget: HTMLInputElement }) => void;
		onblur?: (event: FocusEvent & { currentTarget: HTMLInputElement }) => void;
	}

	type AutoFill =
		| 'off'
		| 'on'
		| 'name'
		| 'email'
		| 'username'
		| 'new-password'
		| 'current-password'
		| 'one-time-code'
		| 'organization'
		| 'street-address'
		| 'country'
		| 'country-name'
		| 'postal-code'
		| 'tel'
		| 'url';
</script>

<script lang="ts">
	let {
		type = 'text',
		size = 'md',
		value = $bindable(''),
		placeholder,
		disabled = false,
		readonly = false,
		required = false,
		name,
		id,
		autocomplete,
		error,
		class: className = '',
		oninput,
		onchange,
		onblur
	}: InputProps = $props();

	const baseClasses = `
		w-full rounded-lg border bg-white
		text-secondary-900 placeholder:text-secondary-400
		transition-colors duration-150
		focus:outline-none focus:ring-2 focus:ring-offset-0
		disabled:cursor-not-allowed disabled:opacity-50
		dark:bg-secondary-900 dark:text-secondary-100 dark:placeholder:text-secondary-500
	`;

	const normalClasses = `
		border-secondary-300 
		focus:border-primary-500 focus:ring-primary-500
		dark:border-secondary-600 dark:focus:border-primary-500
	`;

	const errorClasses = `
		border-error-500 
		focus:border-error-500 focus:ring-error-500
	`;

	const sizeClasses: Record<InputSize, string> = {
		sm: 'h-8 px-3 text-sm',
		md: 'h-10 px-3 text-sm',
		lg: 'h-12 px-4 text-base'
	};

	const classes = $derived(
		[baseClasses, error ? errorClasses : normalClasses, sizeClasses[size], className]
			.join(' ')
			.replace(/\s+/g, ' ')
			.trim()
	);
</script>

<div class="w-full">
	<input
		{type}
		{name}
		{id}
		{placeholder}
		{disabled}
		{readonly}
		{required}
		autocomplete={autocomplete}
		class={classes}
		bind:value
		aria-invalid={!!error}
		aria-describedby={error ? `${id}-error` : undefined}
		{oninput}
		{onchange}
		{onblur}
	/>
	{#if error}
		<p id="{id}-error" class="mt-1.5 text-sm text-error-600 dark:text-error-500">
			{error}
		</p>
	{/if}
</div>
