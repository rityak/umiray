import { type Feature, has } from "../features";

/// Render the children only where the OS has the feature (D-174).
export default function Supported({
  feature,
  children,
}: {
  feature: Feature;
  children: React.ReactNode;
}) {
  return has(feature) ? children : null;
}
