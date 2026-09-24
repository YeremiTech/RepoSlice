import PngImage from "./PngImage";
import { assetUrl } from "../lib/technologyIcons";
import catalog from "../lib/assets.json" with { type: "json" };
type IconProps = {size?: number; className?: string};
function Icon({name, ...props}: IconProps & {name: keyof typeof catalog.icons}) { return <PngImage src={assetUrl(`${catalog.icons[name]}.png`)} alt="" {...props}/>; }
export const DashboardIcon = (props: IconProps) => <Icon name="Dashboard" {...props}/>;
export const StackIcon = (props: IconProps) => <Icon name="Stack" {...props}/>;
export const ApiIcon = (props: IconProps) => <Icon name="Api" {...props}/>;
export const FolderIcon = (props: IconProps) => <Icon name="Folder" {...props}/>;
export const GitHubIcon = (props: IconProps) => <PngImage src={assetUrl("actions/github-logo.png")} alt="" {...props}/>;
export const ScanIcon = (props: IconProps) => <Icon name="Scan" {...props}/>;
export const CodeIcon = (props: IconProps) => <Icon name="Code" {...props}/>;
export const LinkIcon = (props: IconProps) => <Icon name="Link" {...props}/>;
export const ServerIcon = (props: IconProps) => <Icon name="Server" {...props}/>;
export const GlobeIcon = (props: IconProps) => <Icon name="Globe" {...props}/>;
export const DatabaseIcon = (props: IconProps) => <Icon name="Database" {...props}/>;
