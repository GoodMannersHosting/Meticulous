import type { HandleClientError } from '@sveltejs/kit';

export const handleError: HandleClientError = ({ error, event, status, message }) => {
	// Log errors to console in development
	if (import.meta.env.DEV) {
		console.error('Client error:', { error, event, status, message });
	}

	// TODO: Send errors to telemetry service in production
	// if (import.meta.env.PROD) {
	//   sendToTelemetry({ error, event, status, message });
	// }

	// Return user-friendly error message
	const errorMessage = error instanceof Error ? error.message : message;

	return {
		message: errorMessage || 'An unexpected error occurred',
		code: status?.toString()
	};
};
