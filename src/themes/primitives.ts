import { elevation } from "./elevation";
import { effects } from "./effects";
import { layout } from "./layout";
import { motion } from "./motion";
import { shape } from "./shape";
import { sizing } from "./sizing";
import { spacing } from "./spacing";
import { typography } from "./typography";

export const primitives = {
  typography,
  spacing,
  sizing,
  shape,
  elevation,
  effects,
  motion,
  layout,
} as const;

export type ThemePrimitives = typeof primitives;
