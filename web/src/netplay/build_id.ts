/** Which NESER a browser runs: two browsers play together only when these match exactly. */
export type BuildId = { version: string; commit: string };

/** This page's build, stamped in by Vite from Cargo.toml and the commit it was built from. */
export const THIS_BUILD: BuildId = { version: __NESER_VERSION__, commit: __NESER_COMMIT__ };

/** A build as players read it: "1.3.0 (4f2a9c1)". */
export function buildLabel(build: BuildId): string {
    return `${build.version} (${build.commit})`;
}

export function sameBuild(a: BuildId, b: BuildId): boolean {
    return a.version === b.version && a.commit === b.commit;
}
