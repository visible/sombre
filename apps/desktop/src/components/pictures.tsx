import { convertFileSrc } from "@tauri-apps/api/core";

const source = (image: string) =>
  image.startsWith("data:") ? image : convertFileSrc(image);

export const Pictures = ({ images }: { images: string[] }) =>
  images.length === 0 ? null : (
    <div className="mt-2 flex flex-wrap gap-2">
      {images.map((image, index) => (
        <img
          alt={`attachment ${index + 1}`}
          className="border-rim h-28 max-w-56 rounded-lg border object-cover"
          decoding="async"
          key={image}
          loading="lazy"
          src={source(image)}
        />
      ))}
    </div>
  );
