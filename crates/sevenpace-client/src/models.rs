//! Serde DTOs mirroring 7pace JSON (camelCase).
//!
//! - `Envelope<T>`: every response is `{ "data": T }` or `{ "error": { code, message } }`.
//! - `WorkLog`: `id` (uuid), `timeStamp` (start, user-local time), `length` / `billableLength`
//!   (seconds), `workItemId`, `comment`, `user`, `activityType`, `createdTimestamp`,
//!   `editedTimestamp`, `isCanEdit` / `isCanDelete` (with `$includeEditable`).
//! - `WorkLogCreate`: `timeStamp`, `length`, `billableLength?`, `workItemId?`, `comment?`
//!   (one of the two is required), `userId?`, `activityTypeId?`. In the *batch* schema
//!   `workItemId` is a string, not an int.
//! - `ActivityTypes`: `isEnabled`, `defaultActivity`, `activityTypes: [{ id, name, color }]`.
//! - `TrackingStart`: `tfsId`, `timeZone` (offset in minutes), `remark`, `activityTypeId`,
//!   `isBillable`.
