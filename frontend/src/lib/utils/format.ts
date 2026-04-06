import { formatDistanceToNow, format, formatDuration, intervalToDuration } from 'date-fns';

export function formatRelativeTime(date: string | Date): string {
	const d = typeof date === 'string' ? new Date(date) : date;
	return formatDistanceToNow(d, { addSuffix: true });
}

export function formatDateTime(date: string | Date): string {
	const d = typeof date === 'string' ? new Date(date) : date;
	return format(d, 'MMM d, yyyy h:mm a');
}

/** Local absolute time for native `title` tooltips; empty if missing/invalid. */
export function formatDateTimeForTitle(date: string | Date | undefined | null): string {
	if (date == null || date === '') return '';
	const d = typeof date === 'string' ? new Date(date) : date;
	if (Number.isNaN(d.getTime())) return typeof date === 'string' ? date : '';
	return formatDateTime(d);
}

export function formatDateShort(date: string | Date): string {
	const d = typeof date === 'string' ? new Date(date) : date;
	return format(d, 'MMM d, yyyy');
}

export function formatTimeShort(date: string | Date): string {
	const d = typeof date === 'string' ? new Date(date) : date;
	return format(d, 'h:mm a');
}

export function formatDurationMs(ms: number | null | undefined): string {
	if (ms == null) return '—';

	const duration = intervalToDuration({ start: 0, end: ms });

	if (ms < 1000) {
		return `${ms}ms`;
	}

	if (ms < 60000) {
		return `${Math.round(ms / 1000)}s`;
	}

	const parts: string[] = [];
	if (duration.hours) parts.push(`${duration.hours}h`);
	if (duration.minutes) parts.push(`${duration.minutes}m`);
	if (duration.seconds) parts.push(`${duration.seconds}s`);

	return parts.join(' ') || '0s';
}

export function formatDurationSeconds(seconds: number | null | undefined): string {
	if (seconds == null) return '—';
	return formatDurationMs(seconds * 1000);
}

export function truncateId(id: string, length: number = 8): string {
	if (id.length <= length) return id;
	return id.slice(0, length);
}

export function formatNumber(n: number): string {
	return new Intl.NumberFormat().format(n);
}

export function formatBytes(bytes: number): string {
	if (bytes === 0) return '0 B';

	const k = 1024;
	const sizes = ['B', 'KB', 'MB', 'GB', 'TB'];
	const i = Math.floor(Math.log(bytes) / Math.log(k));

	return `${parseFloat((bytes / Math.pow(k, i)).toFixed(1))} ${sizes[i]}`;
}

export function pluralize(count: number, singular: string, plural?: string): string {
	return count === 1 ? singular : (plural ?? singular + 's');
}
