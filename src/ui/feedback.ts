/** Type transversal minimal, extrait de CommandBar.tsx pour que `src/commands/*.ts` (qui doit
 * produire du feedback) n'ait pas à dépendre d'un composant UI pour un simple type de données. */

export type FeedbackKind = "info" | "success" | "error";

export interface Feedback {
  kind: FeedbackKind;
  message: string;
}
