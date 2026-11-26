// Color World: recolour the screen. Uniforms (GLDEFS, all int): mode,
// tics (map time, 35 per second), speed and strength in percent.
// GZDoom / UZDoom post-process GLSL; Room translates it to WGSL.

vec3 hsv2rgb(vec3 c)
{
	vec3 p = abs(fract(c.xxx + vec3(1.0, 2.0 / 3.0, 1.0 / 3.0)) * 6.0 - 3.0);
	return c.z * mix(vec3(1.0), clamp(p - 1.0, 0.0, 1.0), c.y);
}

vec3 rgb2hsv(vec3 c)
{
	vec4 k = vec4(0.0, -1.0 / 3.0, 2.0 / 3.0, -1.0);
	vec4 p = mix(vec4(c.bg, k.wz), vec4(c.gb, k.xy), step(c.b, c.g));
	vec4 q = mix(vec4(p.xyw, c.r), vec4(c.r, p.yzx), step(p.x, c.r));
	float d = q.x - min(q.w, q.y);
	float e = 1.0e-6;
	return vec3(abs(q.z + (q.w - q.y) / (6.0 * d + e)), d / (q.x + e), q.x);
}

float luma(vec3 c)
{
	return dot(c, vec3(0.299, 0.587, 0.114));
}

// 1: only rainbow colours. Brightness picks the hue, which also sweeps
// across the screen and drifts with time; dark stays dark.
vec3 rainbow(vec3 c, vec2 uv, float t)
{
	float l = luma(c);
	float hue = fract(l * 1.6 + (uv.x + uv.y) * 0.35 + t * 0.08);
	return hsv2rgb(vec3(hue, 0.9, clamp(sqrt(l) * 1.15, 0.0, 1.0)));
}

// 2: seven flat rainbow bands by brightness (red … violet), slowly rolling.
vec3 rainbowBands(vec3 c, float t)
{
	float l = luma(c);
	float band = floor(clamp(l, 0.0, 0.999) * 7.0);
	float hue = fract(band / 7.0 * 0.83 + t * 0.03);
	return hsv2rgb(vec3(hue, 1.0, 0.35 + 0.65 * l));
}

// 3: keep the picture, turn every hue around the colour wheel.
vec3 hueCycle(vec3 c, float t)
{
	vec3 hsv = rgb2hsv(c);
	hsv.x = fract(hsv.x + t * 0.1);
	hsv.y = max(hsv.y, 0.45);
	return hsv2rgb(hsv);
}

vec3 sepia(vec3 c)
{
	return clamp(vec3(
		dot(c, vec3(0.393, 0.769, 0.189)),
		dot(c, vec3(0.349, 0.686, 0.168)),
		dot(c, vec3(0.272, 0.534, 0.131))), 0.0, 1.0);
}

// 7: heat camera: black, blue, magenta, red, yellow, white.
vec3 thermal(vec3 c)
{
	float l = clamp(luma(c), 0.0, 1.0);
	vec3 a = mix(vec3(0.0, 0.0, 0.1), vec3(0.1, 0.1, 0.9), clamp(l * 5.0, 0.0, 1.0));
	a = mix(a, vec3(0.8, 0.1, 0.8), clamp(l * 5.0 - 1.0, 0.0, 1.0));
	a = mix(a, vec3(1.0, 0.15, 0.1), clamp(l * 5.0 - 2.0, 0.0, 1.0));
	a = mix(a, vec3(1.0, 0.9, 0.1), clamp(l * 5.0 - 3.0, 0.0, 1.0));
	return mix(a, vec3(1.0), clamp(l * 5.0 - 4.0, 0.0, 1.0));
}

// 8: the four greens of an old handheld.
vec3 gameBoy(vec3 c)
{
	float l = clamp(luma(c), 0.0, 0.999);
	float shade = floor(l * 4.0);
	if (shade < 1.0) return vec3(0.06, 0.22, 0.06);
	if (shade < 2.0) return vec3(0.19, 0.38, 0.19);
	if (shade < 3.0) return vec3(0.55, 0.67, 0.06);
	return vec3(0.61, 0.74, 0.06);
}

// 9: glowing rainbow outlines on black.
vec3 neonEdges(vec2 uv, float t)
{
	vec2 px = 1.0 / vec2(textureSize(InputTexture, 0));
	float tl = luma(texture(InputTexture, uv + px * vec2(-1.0, -1.0)).rgb);
	float tc = luma(texture(InputTexture, uv + px * vec2(0.0, -1.0)).rgb);
	float tr = luma(texture(InputTexture, uv + px * vec2(1.0, -1.0)).rgb);
	float ml = luma(texture(InputTexture, uv + px * vec2(-1.0, 0.0)).rgb);
	float mr = luma(texture(InputTexture, uv + px * vec2(1.0, 0.0)).rgb);
	float bl = luma(texture(InputTexture, uv + px * vec2(-1.0, 1.0)).rgb);
	float bc = luma(texture(InputTexture, uv + px * vec2(0.0, 1.0)).rgb);
	float br = luma(texture(InputTexture, uv + px * vec2(1.0, 1.0)).rgb);
	float gx = (tr + 2.0 * mr + br) - (tl + 2.0 * ml + bl);
	float gy = (bl + 2.0 * bc + br) - (tl + 2.0 * tc + tr);
	float edge = clamp(sqrt(gx * gx + gy * gy) * 3.0, 0.0, 1.0);
	float hue = fract((uv.x - uv.y) * 0.5 + t * 0.1);
	return hsv2rgb(vec3(hue, 0.8, 1.0)) * edge;
}

void main()
{
	vec4 src = texture(InputTexture, TexCoord);
	vec3 c = src.rgb;
	float t = float(tics) / 35.0 * float(speed) / 100.0;
	vec3 o = c;
	if (mode == 1) o = rainbow(c, TexCoord, t);
	else if (mode == 2) o = rainbowBands(c, t);
	else if (mode == 3) o = hueCycle(c, t);
	else if (mode == 4) o = vec3(luma(c));
	else if (mode == 5) o = sepia(c);
	else if (mode == 6) o = vec3(1.0) - c;
	else if (mode == 7) o = thermal(c);
	else if (mode == 8) o = gameBoy(c);
	else if (mode == 9) o = neonEdges(TexCoord, t);
	FragColor = vec4(mix(c, o, clamp(float(strength) / 100.0, 0.0, 1.0)), src.a);
}
