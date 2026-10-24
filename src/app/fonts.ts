import {
  Castoro_Titling,
  Fragment_Mono,
  Host_Grotesk,
  Libre_Caslon_Display,
  Libre_Caslon_Text,
} from "next/font/google";

export const hostGrotesk = Host_Grotesk({
  subsets: ["latin"],
  variable: "--font-host-grotesk",
});

export const libreCaslonDisplay = Libre_Caslon_Display({
  subsets: ["latin"],
  weight: "400",
  variable: "--font-libre-caslon-display",
});

export const libreCaslonText = Libre_Caslon_Text({
  subsets: ["latin"],
  weight: "400",
  style: ["normal", "italic"],
  variable: "--font-libre-caslon-text",
});

export const castoroTitling = Castoro_Titling({
  subsets: ["latin"],
  weight: "400",
  variable: "--font-castoro-titling",
});

export const fragmentMono = Fragment_Mono({
  subsets: ["latin"],
  weight: "400",
  variable: "--font-fragment-mono",
});

export const fontVariables = [
  hostGrotesk.variable,
  libreCaslonDisplay.variable,
  libreCaslonText.variable,
  castoroTitling.variable,
  fragmentMono.variable,
].join(" ");
