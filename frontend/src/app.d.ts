/// <reference types="@sveltejs/kit" />

declare global {
	namespace App {
		interface Error {
			message: string;
			code?: string;
		}
		interface Locals {
			user?: {
				id: string;
				name: string;
				email: string;
				avatar?: string;
			};
		}
		interface PageData {
			title?: string;
			breadcrumbs?: Array<{ label: string; href?: string }>;
		}
		interface PageState {}
		interface Platform {}
	}
}

export {};
