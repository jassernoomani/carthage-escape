/**
 * Compile-time check: every code the server can send has English text. If the Rust side adds an
 * error code or status (ts-rs regenerates the union types), `npm run build` fails here until a
 * translation is added to en.json. Nothing in this file runs.
 */
import type { ApiErrorKind } from '../net/api'
import type { RoomStatus } from '../generated/RoomStatus'
import type { WsErrorCode } from '../generated/WsErrorCode'
import type en from './en.json'

/** `true` if every code is a key of the en.json section, otherwise `never` (a type error). */
type Covers<Codes extends string, Section> = [Codes] extends [keyof Section] ? true : never

export const apiErrorsTranslated: Covers<ApiErrorKind, (typeof en)['errors']> = true
export const wsErrorsTranslated: Covers<WsErrorCode, (typeof en)['ws_errors']> = true
export const statusesTranslated: Covers<RoomStatus, (typeof en)['status']> = true
