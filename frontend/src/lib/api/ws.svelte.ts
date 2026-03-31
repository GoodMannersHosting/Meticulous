import { browser } from '$app/environment';
import type { WsMessage, WsMessageType } from './types';

export type ConnectionState = 'connecting' | 'connected' | 'disconnected' | 'reconnecting';

type MessageHandler<T = unknown> = (message: WsMessage<T>) => void;
type StateChangeHandler = (state: ConnectionState) => void;

interface WebSocketManagerOptions {
	url: string;
	reconnectDelay?: number;
	maxReconnectDelay?: number;
	heartbeatInterval?: number;
	heartbeatTimeout?: number;
}

class WebSocketManager {
	private ws: WebSocket | null = null;
	private url: string;
	private reconnectDelay: number;
	private maxReconnectDelay: number;
	private heartbeatInterval: number;
	private heartbeatTimeout: number;
	private currentReconnectDelay: number;
	private reconnectTimer: ReturnType<typeof setTimeout> | null = null;
	private heartbeatTimer: ReturnType<typeof setInterval> | null = null;
	private heartbeatTimeoutTimer: ReturnType<typeof setTimeout> | null = null;
	private messageHandlers: Map<WsMessageType | '*', Set<MessageHandler>> = new Map();
	private stateHandlers: Set<StateChangeHandler> = new Set();
	private _state: ConnectionState = 'disconnected';
	private intentionalClose = false;

	constructor(options: WebSocketManagerOptions) {
		this.url = options.url;
		this.reconnectDelay = options.reconnectDelay ?? 1000;
		this.maxReconnectDelay = options.maxReconnectDelay ?? 30000;
		this.heartbeatInterval = options.heartbeatInterval ?? 30000;
		this.heartbeatTimeout = options.heartbeatTimeout ?? 10000;
		this.currentReconnectDelay = this.reconnectDelay;
	}

	get state(): ConnectionState {
		return this._state;
	}

	private setState(state: ConnectionState): void {
		if (this._state !== state) {
			this._state = state;
			this.stateHandlers.forEach((handler) => handler(state));
		}
	}

	connect(): void {
		if (!browser) return;
		if (this.ws?.readyState === WebSocket.OPEN) return;

		this.intentionalClose = false;
		this.setState('connecting');

		try {
			const token = localStorage.getItem('auth_token');
			const wsUrl = token ? `${this.url}?token=${encodeURIComponent(token)}` : this.url;

			this.ws = new WebSocket(wsUrl);
			this.setupEventHandlers();
		} catch (error) {
			console.error('WebSocket connection error:', error);
			this.scheduleReconnect();
		}
	}

	disconnect(): void {
		this.intentionalClose = true;
		this.cleanup();
		this.ws?.close(1000, 'Client disconnect');
		this.ws = null;
		this.setState('disconnected');
	}

	private setupEventHandlers(): void {
		if (!this.ws) return;

		this.ws.onopen = () => {
			this.setState('connected');
			this.currentReconnectDelay = this.reconnectDelay;
			this.startHeartbeat();
		};

		this.ws.onclose = (event) => {
			this.cleanup();

			if (!this.intentionalClose) {
				console.log(`WebSocket closed: ${event.code} ${event.reason}`);
				this.scheduleReconnect();
			} else {
				this.setState('disconnected');
			}
		};

		this.ws.onerror = (error) => {
			console.error('WebSocket error:', error);
		};

		this.ws.onmessage = (event) => {
			try {
				const message = JSON.parse(event.data) as WsMessage;
				this.handleMessage(message);
			} catch (error) {
				console.error('Failed to parse WebSocket message:', error);
			}
		};
	}

	private handleMessage(message: WsMessage): void {
		if (message.type === 'pong') {
			this.clearHeartbeatTimeout();
			return;
		}

		// Call type-specific handlers
		const typeHandlers = this.messageHandlers.get(message.type);
		typeHandlers?.forEach((handler) => handler(message));

		// Call wildcard handlers
		const wildcardHandlers = this.messageHandlers.get('*');
		wildcardHandlers?.forEach((handler) => handler(message));
	}

	private startHeartbeat(): void {
		this.stopHeartbeat();

		this.heartbeatTimer = setInterval(() => {
			if (this.ws?.readyState === WebSocket.OPEN) {
				this.send({ type: 'ping', payload: null, timestamp: new Date().toISOString() });
				this.startHeartbeatTimeout();
			}
		}, this.heartbeatInterval);
	}

	private stopHeartbeat(): void {
		if (this.heartbeatTimer) {
			clearInterval(this.heartbeatTimer);
			this.heartbeatTimer = null;
		}
		this.clearHeartbeatTimeout();
	}

	private startHeartbeatTimeout(): void {
		this.clearHeartbeatTimeout();

		this.heartbeatTimeoutTimer = setTimeout(() => {
			console.warn('Heartbeat timeout - reconnecting');
			this.ws?.close(4000, 'Heartbeat timeout');
		}, this.heartbeatTimeout);
	}

	private clearHeartbeatTimeout(): void {
		if (this.heartbeatTimeoutTimer) {
			clearTimeout(this.heartbeatTimeoutTimer);
			this.heartbeatTimeoutTimer = null;
		}
	}

	private scheduleReconnect(): void {
		if (this.intentionalClose) return;

		this.setState('reconnecting');

		this.reconnectTimer = setTimeout(() => {
			this.reconnectTimer = null;
			this.connect();
		}, this.currentReconnectDelay);

		// Exponential backoff
		this.currentReconnectDelay = Math.min(this.currentReconnectDelay * 2, this.maxReconnectDelay);
	}

	private cleanup(): void {
		this.stopHeartbeat();

		if (this.reconnectTimer) {
			clearTimeout(this.reconnectTimer);
			this.reconnectTimer = null;
		}
	}

	send(message: WsMessage): void {
		if (this.ws?.readyState === WebSocket.OPEN) {
			this.ws.send(JSON.stringify(message));
		}
	}

	on<T = unknown>(type: WsMessageType | '*', handler: MessageHandler<T>): () => void {
		let handlers = this.messageHandlers.get(type);
		if (!handlers) {
			handlers = new Set();
			this.messageHandlers.set(type, handlers);
		}
		handlers.add(handler as MessageHandler);

		return () => {
			handlers?.delete(handler as MessageHandler);
			if (handlers?.size === 0) {
				this.messageHandlers.delete(type);
			}
		};
	}

	onStateChange(handler: StateChangeHandler): () => void {
		this.stateHandlers.add(handler);
		return () => {
			this.stateHandlers.delete(handler);
		};
	}
}

// Create singleton instance
let wsManager: WebSocketManager | null = null;

export function getWebSocketManager(): WebSocketManager {
	if (!wsManager && browser) {
		const wsUrl = import.meta.env.PUBLIC_WS_URL ?? 'ws://localhost:8080/ws';
		wsManager = new WebSocketManager({ url: wsUrl });
	}
	return wsManager!;
}

export function createConnectionStateStore() {
	let state = $state<ConnectionState>('disconnected');

	if (browser) {
		const manager = getWebSocketManager();
		state = manager.state;
		manager.onStateChange((newState) => {
			state = newState;
		});
	}

	return {
		get current() {
			return state;
		}
	};
}
