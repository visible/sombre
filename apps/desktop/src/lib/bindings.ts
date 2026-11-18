import { invoke as __TAURI_INVOKE, Channel } from "@tauri-apps/api/core";
import type { InvokeOptions } from "@tauri-apps/api/core";

export const commands = {
	chats: (options?: InvokeOptions) => __TAURI_INVOKE<Chat[]>("chats", undefined, options),
	chat: (agent: Agent, id: string, whole: boolean, options?: InvokeOptions) => __TAURI_INVOKE<Story>("chat", { agent, id, whole }, options),
	defaults: (options?: InvokeOptions) => __TAURI_INVOKE<Defaults>("defaults", undefined, options),
	folder: (path: string, options?: InvokeOptions) => __TAURI_INVOKE<Folder>("folder", { path }, options),
	models: (agent: Agent, options?: InvokeOptions) => __TAURI_INVOKE<Model[]>("models", { agent }, options),
	start: (chat: Opening, updates: Channel<Update>, options?: InvokeOptions) => __TAURI_INVOKE<null>("start", { chat, updates }, options),
	send: (key: string, text: string, images: string[], options?: InvokeOptions) => __TAURI_INVOKE<null>("send", { key, text, images }, options),
	steer: (key: string, text: string, options?: InvokeOptions) => __TAURI_INVOKE<boolean>("steer", { key, text }, options),
	answer: (key: string, id: string, choice: Choice, options?: InvokeOptions) => __TAURI_INVOKE<null>("answer", { key, id, choice }, options),
	reply: (key: string, id: string, answers: string[] | null, options?: InvokeOptions) => __TAURI_INVOKE<null>("reply", { key, id, answers }, options),
	pair: (key: string, home: boolean | null, options?: InvokeOptions) => __TAURI_INVOKE<Paired>("pair", { key, home }, options),
	phones: (options?: InvokeOptions) => __TAURI_INVOKE<Phone[]>("phones", undefined, options),
	forget: (id: string, options?: InvokeOptions) => __TAURI_INVOKE<boolean>("forget", { id }, options),
	unpair: (options?: InvokeOptions) => __TAURI_INVOKE<boolean>("unpair", undefined, options),
	followed: (options?: InvokeOptions) => __TAURI_INVOKE<string | null>("followed", undefined, options),
	stop: (key: string, options?: InvokeOptions) => __TAURI_INVOKE<null>("stop", { key }, options),
	tune: (key: string, model: string | null, mode: Mode | null, effort: string | null, options?: InvokeOptions) => __TAURI_INVOKE<string>("tune", { key, model, mode, effort }, options),
	rename: (key: string, name: string, options?: InvokeOptions) => __TAURI_INVOKE<string>("rename", { key, name }, options),
	compact: (key: string, options?: InvokeOptions) => __TAURI_INVOKE<null>("compact", { key }, options),
	keep: (data: string, options?: InvokeOptions) => __TAURI_INVOKE<Picture>("keep", { data }, options),
	picture: (path: string, options?: InvokeOptions) => __TAURI_INVOKE<Picture>("picture", { path }, options),
	settings: (folder: string | null, options?: InvokeOptions) => __TAURI_INVOKE<Settings>("settings", { folder }, options),
	configure: (change: Change, folder: string | null, options?: InvokeOptions) => __TAURI_INVOKE<Settings>("configure", { change, folder }, options),
	connect: (name: string, url: string, key: string, folder: string | null, options?: InvokeOptions) => __TAURI_INVOKE<Settings>("connect", { name, url, key, folder }, options),
	rekey: (name: string, key: string, folder: string | null, options?: InvokeOptions) => __TAURI_INVOKE<Settings>("rekey", { name, key, folder }, options),
	disconnect: (name: string, folder: string | null, options?: InvokeOptions) => __TAURI_INVOKE<Settings>("disconnect", { name, folder }, options),
	plug: (name: string, run: string, folder: string | null, options?: InvokeOptions) => __TAURI_INVOKE<Settings>("plug", { name, run, folder }, options),
	unplug: (name: string, folder: string | null, options?: InvokeOptions) => __TAURI_INVOKE<Settings>("unplug", { name, folder }, options),
	bring: (name: string, folder: string | null, options?: InvokeOptions) => __TAURI_INVOKE<Settings>("bring", { name, folder }, options),
	goal: (key: string, text: string, options?: InvokeOptions) => __TAURI_INVOKE<null>("goal", { key, text }, options),
	carry: (key: string, from: Agent, id: string, to: Agent, folder: string, options?: InvokeOptions) => __TAURI_INVOKE<Carried>("carry", { key, from, id, to, folder }, options),
	purge: (folder: string, options?: InvokeOptions) => __TAURI_INVOKE<number>("purge", { folder }, options),
	record: (agent: Agent, id: string, change: Record, options?: InvokeOptions) => __TAURI_INVOKE<null>("record", { agent, id, change }, options),
	changes: (folder: string, from: string | null, to: string | null, options?: InvokeOptions) => __TAURI_INVOKE<Diff>("changes", { folder, from, to }, options),
	editors: (options?: InvokeOptions) => __TAURI_INVOKE<Editor[]>("editors", undefined, options),
	launch: (path: string, editor: string | null, options?: InvokeOptions) => __TAURI_INVOKE<null>("launch", { path, editor }, options),
	repo: (folder: string, options?: InvokeOptions) => __TAURI_INVOKE<Repo | null>("repo", { folder }, options),
	files: (folder: string, options?: InvokeOptions) => __TAURI_INVOKE<string[]>("files", { folder }, options),
	commit: (folder: string, message: string, options?: InvokeOptions) => __TAURI_INVOKE<string>("commit", { folder, message }, options),
	push: (folder: string, options?: InvokeOptions) => __TAURI_INVOKE<string>("push", { folder }, options),
	branch: (folder: string, name: string, options?: InvokeOptions) => __TAURI_INVOKE<string>("branch", { folder, name }, options),
	request: (folder: string, options?: InvokeOptions) => __TAURI_INVOKE<string>("request", { folder }, options),
	agents: (options?: InvokeOptions) => __TAURI_INVOKE<Installed[]>("agents", undefined, options),
	pty: (key: string, folder: string, cols: number, rows: number, output: Channel<Out>, options?: InvokeOptions) => __TAURI_INVOKE<null>("pty", { key, folder, cols, rows, output }, options),
	input: (key: string, data: string, options?: InvokeOptions) => __TAURI_INVOKE<null>("input", { key, data }, options),
	resize: (key: string, cols: number, rows: number, options?: InvokeOptions) => __TAURI_INVOKE<null>("resize", { key, cols, rows }, options),
	hangup: (key: string, options?: InvokeOptions) => __TAURI_INVOKE<void>("hangup", { key }, options),
	shot: (name: string, options?: InvokeOptions) => __TAURI_INVOKE<string>("shot", { name }, options),
};

export type Agent = "claude" | "codex" | "sombre";

export type Aim = {
	objective: string,
	status: string,
	active: boolean,
	summary: string,
};

export type Carried = {
	session: string,
	note: string,
};

export type Change = {
	agent?: Agent | null,
	mode?: Mode | null,
	effort?: string | null,
	publish?: boolean | null,
	steer?: boolean | null,
	awake?: boolean | null,
	notify?: boolean | null,
	model?: string | null,
	editor?: string | null,
};

export type Changed = {
	path: string,
	added: number | null,
	removed: number | null,
};

export type Chat = {
	agent: Agent,
	id: string,
	title: string,
	project: string,
	place: string,
	folder: string,
	ago: string,
	updated: number,
};

export type Choice = "yes" | "always" | "no";

export type Defaults = {
	agent: Agent,
	mode: Mode,
	folders: Folder[],
};

export type Diff = {
	from: string,
	to: string,
	text: string,
	cut: boolean,
};

export type Editor = {
	id: string,
	name: string,
};

export type Entry = {
	id: number,
} & Item;

export type File = {
	path: string,
	added: number | null,
	removed: number | null,
};

export type Folder = {
	path: string,
	place: string,
	project: string,
};

export type Installed = {
	agent: Agent,
	version: string | null,
	install: string,
	docs: string,
};

export type Item = { kind: "user"; text: string; images?: string[] } | { kind: "text"; text: string } | { kind: "step"; step: string; label: string; detail: string; output: string; failed: boolean; patch: string[] } | { kind: "plan"; tasks: Task[] } | { kind: "note"; text: string } | { kind: "failed"; text: string } | { kind: "changes"; from: string; to: string; files: File[] } | { kind: "done"; millis: number | null; cost: number | null };

export type Mode = "ask" | "auto" | "yolo";

export type Model = {
	id: string,
	about: string,
};

export type Offer = {
	from: string,
	name: string,
	run: string,
};

export type Opening = {
	key: string,
	agent: Agent,
	id: string | null,
	folder: string | null,
	model: string | null,
	mode: Mode | null,
	effort: string | null,
};

export type Option = {
	label: string,
	about: string,
};

export type Out = { kind: "data"; text: string } | { kind: "exit"; code: number | null };

export type Paired = {
	link: string,
	code: string,
	reach: string,
	home: boolean,
};

export type Phone = {
	id: string,
	label: string,
	since: number,
};

export type Picture = {
	path: string,
	src: string,
};

export type Preset = {
	id: string,
	label: string,
	url: string,
	local: boolean,
};

export type Provider = {
	name: string,
	url: string,
	key: string | null,
	builtin: boolean,
};

export type Question = {
	header: string,
	text: string,
	choices: Option[],
	multiple: boolean,
};

export type Record = {
	turn: number,
	from: string,
	to: string,
	files: File[],
};

export type Repo = {
	branch: string | null,
	upstream: string | null,
	ahead: number,
	behind: number,
	changed: number,
	files: string[],
	remote: string | null,
};

export type Server = {
	name: string,
	run: string,
};

export type Settings = {
	agent: Agent,
	mode: Mode,
	effort: string | null,
	publish: boolean,
	steer: boolean,
	awake: boolean,
	notify: boolean,
	model: string,
	editor: string | null,
	editors: Editor[],
	providers: Provider[],
	presets: Preset[],
	servers: Server[],
	offered: Offer[],
	skills: Skill[],
	home: string,
};

export type Skill = {
	name: string,
	about: string,
};

export type Story = {
	entries: Entry[],
	whole: boolean,
};

export type Task = {
	text: string,
	status: string,
};

export type Update = { kind: "state"; busy: boolean; model: string | null; mode: Mode; session: string | null; effort: string | null; levels: string[]; cost: number | null; left: number | null; folder: string; goal: Aim | null } | { kind: "delta"; text: string } | { kind: "text"; text: string } | { kind: "step"; step: string; label: string; detail: string } | { kind: "output"; step: string; text: string; failed: boolean } | { kind: "patch"; step: string; lines: string[] } | { kind: "plan"; tasks: Task[] } | { kind: "ask"; id: string; tool: string; detail: string; reason: string; change: string[]; always: string | null } | { kind: "questions"; id: string; questions: Question[] } | { kind: "granted"; id: string } | { kind: "phone"; text: string } | { kind: "decided"; id: string; yes: boolean } | { kind: "replied"; id: string; answers: string[] | null } | { kind: "note"; text: string } | { kind: "changes"; from: string; to: string; files: Changed[] } | { kind: "failed"; text: string } | { kind: "done"; millis: number | null; cost: number | null; left: number | null };
