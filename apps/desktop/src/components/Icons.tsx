import PngImage from "./PngImage";
import { assetUrl, fallbackUrl, technologyIconUrl } from "../lib/technologyIcons";

type IconProps = { size?: number };
const icons = {
  OverviewIcon: "navigation/summary.png",
  ProjectsIcon: "actions/folder.png",
  ComponentsIcon: "navigation/technologies.png",
  EntrypointsIcon: "navigation/endpoints.png",
  DependenciesIcon: "status/link.png",
  ArchitectureIcon: "navigation/summary.png",
  AuditIcon: "actions/analyze.png",
  DependencyNodeIcon: "status/link.png",
  CapsuleIcon: "actions/folder.png",
  PlusIcon: "actions/analyze.png",
  SearchIcon: "actions/analyze.png",
  ScanIcon: "actions/analyze.png",
  ChevronIcon: "navigation/summary.png",
  FolderIcon: "actions/folder.png",
  FilesIcon: "actions/folder.png",
  MetricComponentsIcon: "navigation/technologies.png",
  MetricEntrypointsIcon: "navigation/endpoints.png",
  GlobeIcon: "navigation/endpoints.png",
  ServerIcon: "navigation/technologies.png",
  LinkIcon: "status/link.png",
} as const;
function Icon({name, size = 20}: IconProps & {name: keyof typeof icons}) {
  return <PngImage src={assetUrl(icons[name])} alt="" size={size}/>;
}
export const OverviewIcon = (props: IconProps) => <Icon name="OverviewIcon" {...props}/>;
export const ProjectsIcon = (props: IconProps) => <Icon name="ProjectsIcon" {...props}/>;
export const ComponentsIcon = (props: IconProps) => <Icon name="ComponentsIcon" {...props}/>;
export const EntrypointsIcon = (props: IconProps) => <Icon name="EntrypointsIcon" {...props}/>;
export const DependenciesIcon = (props: IconProps) => <Icon name="DependenciesIcon" {...props}/>;
export const ArchitectureIcon = (props: IconProps) => <Icon name="ArchitectureIcon" {...props}/>;
export const AuditIcon = (props: IconProps) => <Icon name="AuditIcon" {...props}/>;
export const DependencyNodeIcon = (props: IconProps) => <Icon name="DependencyNodeIcon" {...props}/>;
export const CapsuleIcon = (props: IconProps) => <Icon name="CapsuleIcon" {...props}/>;
export const PlusIcon = (props: IconProps) => <Icon name="PlusIcon" {...props}/>;
export const SearchIcon = (props: IconProps) => <Icon name="SearchIcon" {...props}/>;
export const ScanIcon = (props: IconProps) => <Icon name="ScanIcon" {...props}/>;
export const ChevronIcon = (props: IconProps) => <Icon name="ChevronIcon" {...props}/>;
export const FolderIcon = (props: IconProps) => <Icon name="FolderIcon" {...props}/>;
export const FilesIcon = (props: IconProps) => <Icon name="FilesIcon" {...props}/>;
export const MetricComponentsIcon = (props: IconProps) => <Icon name="MetricComponentsIcon" {...props}/>;
export const MetricEntrypointsIcon = (props: IconProps) => <Icon name="MetricEntrypointsIcon" {...props}/>;
export const GlobeIcon = (props: IconProps) => <Icon name="GlobeIcon" {...props}/>;
export const ServerIcon = (props: IconProps) => <Icon name="ServerIcon" {...props}/>;
export const LinkIcon = (props: IconProps) => <Icon name="LinkIcon" {...props}/>;
export function RuntimeBrandIcon({name, size = 28}: IconProps & {name: string}) {
  return <PngImage src={technologyIconUrl(name, "runtime")} fallback={fallbackUrl("runtime")} alt="" size={size}/>;
}
export function TechnologyBrandIcon({name, size = 28}: IconProps & {name: string}) {
  return <PngImage src={technologyIconUrl(name)} fallback={fallbackUrl()} alt="" size={size}/>;
}
