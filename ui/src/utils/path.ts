/** Format Windows drive and UNC paths with backslashes, preserving POSIX paths. */
export function formatPathForDisplay(path: string) {
  return /^(?:[a-z]:[\\/]|\\\\)/iu.test(path) ? path.replaceAll('/', '\\') : path;
}
