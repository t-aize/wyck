/**
 * Seule vraie donnée transversale de l'app : `feedback`/`setFeedback`
 * sont consommés par plusieurs hooks indépendants (le suivi ATR, le routeur de commandes, les
 * raccourcis clavier) qui ne peuvent pas se les passer directement entre eux — aucun d'eux ne peut
 * posséder cet état lui-même sans créer une dépendance circulaire. Avant ce Context, `setFeedback`
 * était enfilé en tant qu'opt brut dans chacun d'eux ; ici, 0 saut au lieu de 3.
 */

import {
  createContext,
  type Dispatch,
  type ReactNode,
  type SetStateAction,
  useContext,
  useState,
} from "react";
import type { Feedback } from "../feedback.ts";

interface FeedbackContextValue {
  feedback: Feedback;
  setFeedback: Dispatch<SetStateAction<Feedback>>;
}

const FeedbackReactContext = createContext<FeedbackContextValue | undefined>(undefined);

export function FeedbackProvider({ children }: { children: ReactNode }) {
  const [feedback, setFeedback] = useState<Feedback>({
    kind: "info",
    message: "tapez help pour la liste des commandes",
  });

  return (
    <FeedbackReactContext.Provider value={{ feedback, setFeedback }}>
      {children}
    </FeedbackReactContext.Provider>
  );
}

export function useFeedback(): FeedbackContextValue {
  const ctx = useContext(FeedbackReactContext);
  if (!ctx) throw new Error("useFeedback() doit être utilisé sous <FeedbackProvider>.");
  return ctx;
}
