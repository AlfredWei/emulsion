// Shared drag MIME for dragging one or more photos by `image_id` within the app (M5.5 map view: Filmstrip
// cells are the drag source, LibraryMapView the drop target -- see their own doc comments). A made-up,
// private type string, never a real payload from outside this app, so it doubles as a reliable way for
// LibraryModule's own "drop files to import" handling to tell an internal photo drag apart from a real OS
// file drag and ignore it -- one shared constant rather than three copies of the same string literal, so a
// typo in one place can't silently break that check.
export const IMAGE_IDS_DRAG_MIME = "application/x-emulsion-image-ids";
