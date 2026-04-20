// PII scrubber for Sentry events. Parallels the backend scrubber in
// src/observability/scrubber.rs — same regex, same redaction token, so a
// tag visible in a backend event looks the same when mirrored to the
// frontend project.

import type { Breadcrumb, ErrorEvent } from '@sentry/browser';

const SENSITIVE =
  /(token|authorization|ciphertext|jwt|cookie|bearer|api[_-]?key|secret|password)/i;

function redactRecord<T extends Record<string, unknown>>(obj: T | undefined): T | undefined {
  if (!obj) return obj;
  for (const k of Object.keys(obj)) {
    if (SENSITIVE.test(k)) {
      (obj as Record<string, unknown>)[k] = '[redacted]';
    }
  }
  return obj;
}

export function beforeSend(event: ErrorEvent): ErrorEvent {
  redactRecord(event.extra);
  redactRecord(event.tags as Record<string, unknown> | undefined);
  if (event.request) {
    redactRecord(event.request.headers);
    redactRecord(event.request.env);
    event.request.cookies = undefined;
    if (event.request.query_string && SENSITIVE.test(String(event.request.query_string))) {
      event.request.query_string = '[redacted]';
    }
  }
  return event;
}

export function beforeBreadcrumb(bc: Breadcrumb): Breadcrumb | null {
  redactRecord(bc.data);
  if (bc.message && bc.message.length > 40 && SENSITIVE.test(bc.message)) {
    bc.message = '[redacted message]';
  }
  return bc;
}
