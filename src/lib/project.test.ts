import { expect, it } from "vitest";
import { migrateRepositoryUrl, project, repository } from "./project";

it("upgrades the legacy author default without replacing custom repositories", () => {
  for (const value of [
    undefined,
    "",
    "https://github.com/dBevil-z/",
    "https://github.com/dBevil-z",
    "javascript:bad()",
  ])
    expect(migrateRepositoryUrl(value)).toBe(project.repository);
  expect(migrateRepositoryUrl("https://github.com/example/custom")).toBe(
    "https://github.com/example/custom",
  );
});

it("uses an official repository for every release and support entry point", () => {
  for (const value of [
    project.issues,
    project.releases,
    project.latest,
    project.guide,
    project.roadmap,
  ])
    expect(value.startsWith(project.repository + "/")).toBe(true);
  expect(repository("https://github.com:8443/dBevil-z/FileManager")).toBeNull();
});
