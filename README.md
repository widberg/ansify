# img2ansi
Turn images into ANSI-style art

## Usage

```plaintext
Usage: img2ansi [OPTIONS] --input <INPUT_PATH> --palette <PALETTE_PATH> --blocks <BLOCKS_PATH>

Options:
  -i, --input <INPUT_PATH>      
  -o, --output <OUTPUT_PATH>
  -p, --palette <PALETTE_PATH>
  -b, --blocks <BLOCKS_PATH>
  -w, --width <WIDTH>
  -H, --height <HEIGHT>
  -t, --text
  -h, --help                    Print help information
  -V, --version                 Print version information
```

To generate an image with a 256-color palette, classic style block characters, 256 characters wide, and maintain aspect ratio:

```sh
img2ansi -p ./res/256.yaml -b ./res/classic.yaml -w 256 image -i ./res/cat.jpg -o ./res/out.bmp
```

To generate ANSI text with a 16-color palette, small style block characters, 64 characters high, and maintain aspect ratio:

```sh
img2ansi -p ./res/16.yaml -b ./res/small.yaml -H 64 image -i ./res/cat.jpg --text
```

To generate a gif with an 8-color palette, classic style block characters, 32 characters wide, and force 128 characters high:

```sh
img2ansi -p ./res/8.yaml -b ./res/classic.yaml -w 32 -H 128 gif -i ./res/cat.gif -o ./res/out.gif
```

To generate an image text with a 16-color palette, classic style block characters, and one character per pixel in the original image:

```sh
img2ansi -p ./res/16.yaml -b ./res/classic.yaml image -i ./res/cat.jpg -o ./res/out.bmp
```

To live process the first webcam with a 16-color palette, tiny style block characters, and one character per pixel in the original image:

```sh
img2ansi -p ./res/16.yaml -b ./res/tiny.yaml image -i 0
```

You can copy the existing yaml files and edit them to match your terminal/prefered style if you want.
