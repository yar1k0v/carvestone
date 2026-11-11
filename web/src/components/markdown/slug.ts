import GithubSlugger from "github-slugger";

/**
 * Heading anchors in document order, by the engine's rule (`stone get id#anchor`): lower case,
 * spaces to `-`, other punctuation dropped, a repeat gets `-1`, `-2`.
 */
export function slugs(headings: string[]): string[] {
  const slugger = new GithubSlugger();
  return headings.map((heading) => slugger.slug(heading.trim()));
}
