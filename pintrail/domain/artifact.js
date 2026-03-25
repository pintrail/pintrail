"use strict";

const Ok = (res) => ({ ok: true, res });
const Err = (msg) => ({ ok: false, msg });

const hasOwn = (value, key) => Object.prototype.hasOwnProperty.call(value, key);

const isObject = (value) => {
  if (!value) {
    return false;
  }

  if (typeof value !== "object") {
    return false;
  }

  if (Array.isArray(value)) {
    return false;
  }

  return true;
};

const isNonEmptyString = (value) => {
  if (typeof value !== "string") {
    return false;
  }

  return value.trim().length > 0;
};

const isNumber = (value) => typeof value === "number" && Number.isFinite(value);

const hasCoordinates = (value) => {
  if (!isObject(value)) {
    return false;
  }

  if (!hasOwn(value, "x") || !hasOwn(value, "y")) {
    return false;
  }

  if (!isNumber(value.x) || !isNumber(value.y)) {
    return false;
  }

  return true;
};

const validateText = (value, label) => {
  if (!isNonEmptyString(value)) {
    return Err("Artifact " + label + " must be a non-empty string.");
  }

  return Ok(value.trim());
};

const validateImages = (images) => {
  if (typeof images === "undefined") {
    return Ok([]);
  }

  if (!Array.isArray(images)) {
    return Err("Artifact images must be an array.");
  }

  const cleanedImages = [];

  for (const image of images) {
    if (!isNonEmptyString(image)) {
      return Err("Artifact images must contain only non-empty strings.");
    }

    cleanedImages.push(image.trim());
  }

  return Ok(cleanedImages);
};

const readCoordinatePair = (value, xKey, yKey, label) => {
  if (!isObject(value)) {
    return Err("Artifact data must be an object.");
  }

  const hasX = hasOwn(value, xKey);
  const hasY = hasOwn(value, yKey);

  if (hasX !== hasY) {
    return Err(label + " require both " + xKey + " and " + yKey + ".");
  }

  if (!hasX) {
    return Ok({});
  }

  if (!isNumber(value[xKey]) || !isNumber(value[yKey])) {
    return Err(label + " must use finite numeric values.");
  }

  return Ok({
    x: value[xKey],
    y: value[yKey],
  });
};

const resolveCoordinates = (artifactData, inheritedCoordinates) => {
  const ownCoordinatesResult = readCoordinatePair(
    artifactData,
    "ownX",
    "ownY",
    "Artifact own coordinates",
  );

  if (!ownCoordinatesResult.ok) {
    return ownCoordinatesResult;
  }

  if (hasCoordinates(ownCoordinatesResult.res)) {
    return Ok({
      coordinateSource: "own",
      ownX: ownCoordinatesResult.res.x,
      ownY: ownCoordinatesResult.res.y,
      x: ownCoordinatesResult.res.x,
      y: ownCoordinatesResult.res.y,
    });
  }

  const directCoordinatesResult = readCoordinatePair(
    artifactData,
    "x",
    "y",
    "Artifact coordinates",
  );

  if (!directCoordinatesResult.ok) {
    return directCoordinatesResult;
  }

  if (
    hasCoordinates(directCoordinatesResult.res) &&
    artifactData.coordinateSource !== "inherited"
  ) {
    return Ok({
      coordinateSource: "own",
      ownX: directCoordinatesResult.res.x,
      ownY: directCoordinatesResult.res.y,
      x: directCoordinatesResult.res.x,
      y: directCoordinatesResult.res.y,
    });
  }

  if (hasCoordinates(inheritedCoordinates)) {
    return Ok({
      coordinateSource: "inherited",
      x: inheritedCoordinates.x,
      y: inheritedCoordinates.y,
    });
  }

  if (hasCoordinates(directCoordinatesResult.res)) {
    return Ok({
      coordinateSource: "detached",
      x: directCoordinatesResult.res.x,
      y: directCoordinatesResult.res.y,
    });
  }

  return Ok({
    coordinateSource: "unresolved",
  });
};

let buildArtifact = (artifactData, inheritedCoordinates) =>
  Err("Artifact builder is unavailable.");

const buildChildren = (children, inheritedCoordinates) => {
  if (typeof children === "undefined") {
    return Ok([]);
  }

  if (!Array.isArray(children)) {
    return Err("Artifact children must be an array.");
  }

  const builtChildren = [];
  let index = 0;

  for (const child of children) {
    const childResult = buildArtifact(child, inheritedCoordinates);

    if (!childResult.ok) {
      return Err(
        "Artifact child at index " + index + " is invalid: " + childResult.msg,
      );
    }

    builtChildren.push(childResult.res);
    index += 1;
  }

  return Ok(builtChildren);
};

buildArtifact = (artifactData, inheritedCoordinates) => {
  if (!isObject(artifactData)) {
    return Err("Artifact data must be an object.");
  }

  const nameResult = validateText(artifactData.name, "name");

  if (!nameResult.ok) {
    return nameResult;
  }

  const descriptionResult = validateText(
    artifactData.description,
    "description",
  );

  if (!descriptionResult.ok) {
    return descriptionResult;
  }

  const imagesResult = validateImages(artifactData.images);

  if (!imagesResult.ok) {
    return imagesResult;
  }

  const coordinatesResult = resolveCoordinates(
    artifactData,
    inheritedCoordinates || {},
  );

  if (!coordinatesResult.ok) {
    return coordinatesResult;
  }

  const childCoordinates = hasCoordinates(coordinatesResult.res)
    ? { x: coordinatesResult.res.x, y: coordinatesResult.res.y }
    : {};

  const childrenResult = buildChildren(artifactData.children, childCoordinates);

  if (!childrenResult.ok) {
    return childrenResult;
  }

  const artifact = {
    type: "Artifact",
    name: nameResult.res,
    description: descriptionResult.res,
    images: imagesResult.res,
    children: childrenResult.res,
    coordinateSource: coordinatesResult.res.coordinateSource,
  };

  if (hasOwn(coordinatesResult.res, "x")) {
    artifact.x = coordinatesResult.res.x;
  }

  if (hasOwn(coordinatesResult.res, "y")) {
    artifact.y = coordinatesResult.res.y;
  }

  if (hasOwn(coordinatesResult.res, "ownX")) {
    artifact.ownX = coordinatesResult.res.ownX;
  }

  if (hasOwn(coordinatesResult.res, "ownY")) {
    artifact.ownY = coordinatesResult.res.ownY;
  }

  return Ok(artifact);
};

const addImage = (artifactData, image) => {
  const artifactResult = Artifact(artifactData);

  if (!artifactResult.ok) {
    return artifactResult;
  }

  if (!isNonEmptyString(image)) {
    return Err("Artifact image must be a non-empty string.");
  }

  return Ok({
    ...artifactResult.res,
    images: [...artifactResult.res.images, image.trim()],
  });
};

const addChild = (artifactData, childData) => {
  const artifactResult = Artifact(artifactData);

  if (!artifactResult.ok) {
    return artifactResult;
  }

  const inheritedCoordinates = hasCoordinates(artifactResult.res)
    ? { x: artifactResult.res.x, y: artifactResult.res.y }
    : {};

  const childResult = buildArtifact(childData, inheritedCoordinates);

  if (!childResult.ok) {
    return Err("Unable to add child artifact: " + childResult.msg);
  }

  return Ok({
    ...artifactResult.res,
    children: [...artifactResult.res.children, childResult.res],
  });
};

const withCoordinates = (artifactData, x, y) => {
  const artifactResult = Artifact(artifactData);

  if (!artifactResult.ok) {
    return artifactResult;
  }

  if (!isNumber(x) || !isNumber(y)) {
    return Err("Artifact coordinates must be finite numeric values.");
  }

  return buildArtifact(
    {
      ...artifactResult.res,
      x,
      y,
      ownX: x,
      ownY: y,
      coordinateSource: "own",
    },
    {},
  );
};

export const Artifact = (artifactData) => buildArtifact(artifactData, {});
export const isArtifact = (value) => Ok(Artifact(value).ok);
