interface ResourceInfo {
  name: string;
  type: string;
}

declare interface ImportMeta {
  resources: Array<ResourceInfo> & Record<string, Array<ResourceInfo>>;
}
