const sharp = require('sharp');

const input = process.argv[2];
const whiteOutput = process.argv[3];
const blackOutput = process.argv[4];

async function main() {
  const source = sharp(input);
  const metadata = await source.metadata();
  if (!metadata.hasAlpha) {
    throw new Error('Source image has no alpha channel.');
  }

  const foreground = await source.png().toBuffer();
  const makeBackground = (background, output) =>
    sharp({
      create: {
        width: metadata.width,
        height: metadata.height,
        channels: 4,
        background,
      },
    })
      .composite([{ input: foreground, left: 0, top: 0 }])
      .removeAlpha()
      .png()
      .toFile(output);

  await Promise.all([
    makeBackground({ r: 255, g: 255, b: 255, alpha: 1 }, whiteOutput),
    makeBackground({ r: 0, g: 0, b: 0, alpha: 1 }, blackOutput),
  ]);

  console.log(JSON.stringify({
    width: metadata.width,
    height: metadata.height,
    hasAlpha: metadata.hasAlpha,
    whiteOutput,
    blackOutput,
  }));
}

main().catch((error) => {
  console.error(error);
  process.exit(1);
});
