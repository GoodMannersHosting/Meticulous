import type { HandleClientError } from '@sveltejs/kit';
import { PUBLIC_API_URL } from '$env/static/public';

interface TelemetryPayload {
	type: 'client_error';
	timestamp: string;
	url: string;
	status?: number;
	message: string;
	stack?: string;
	userAgent: string;
	referrer: string;
}

async function sendToTelemetry(payload: TelemetryPayload): Promise<void> {
	const apiUrl = PUBLIC_API_URL || '';
	
	try {
		await fetch(`${apiUrl}/api/v1/telemetry/errors`, {
			method: 'POST',
			headers: {
				'Content-Type': 'application/json'
			},
			body: JSON.stringify(payload),
			keepalive: true
		});
	} catch {
		// Silently fail - telemetry should not affect user experience
	}
}

export const handleError: HandleClientError = ({ error, event, status, message }) => {
	// Log errors to console in development
	if (import.meta.env.DEV) {
		console.error('Client error:', { error, event, status, message });
	}

	// Send errors to telemetry service in production
	if (import.meta.env.PROD) {
		const errorMessage = error instanceof Error ? error.message : message;
		const errorStack = error instanceof Error ? error.stack : undefined;

		sendToTelemetry({
			type: 'client_error',
			timestamp: new Date().toISOString(),
			url: typeof window !== 'undefined' ? window.location.href : '',
			status,
			message: errorMessage || 'Unknown error',
			stack: errorStack,
			userAgent: typeof navigator !== 'undefined' ? navigator.userAgent : '',
			referrer: typeof document !== 'undefined' ? document.referrer : ''
		});
	}

	// Return user-friendly error message
	const errorMessage = error instanceof Error ? error.message : message;

	return {
		message: errorMessage || 'An unexpected error occurred',
		code: status?.toString()
	};
};
