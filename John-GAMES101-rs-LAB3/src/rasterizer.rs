use std::rc::Rc;

use nalgebra::{Matrix4, Vector2, Vector3, Vector4};
use crate::shader::{FragmentShaderPayload, VertexShaderPayload};
use crate::texture::Texture;
use crate::triangle::Triangle;
use crate::utils::{bump_fragment_shader, displacement_fragment_shader, phong_fragment_shader, texture_fragment_shader};

#[allow(dead_code)]
pub enum Buffer {
    Color,
    Depth,
    Both,
}

#[allow(dead_code)]
pub enum Primitive {
    Line,
    Triangle,
}

#[derive(Default)]
pub struct Rasterizer {
    model: Matrix4<f64>,
    view: Matrix4<f64>,
    projection: Matrix4<f64>,
    texture: Option<Texture>,

    vert_shader: Option<fn(&VertexShaderPayload) -> Vector3<f64>>,
    fragment_shader: Option<fn(&FragmentShaderPayload) -> Vector3<f64>>,
    frame_buf: Vec<Vector3<f64>>,
    depth_buf: Vec<f64>,
    //frame_sample: Vec<Vec<(f64,Vector3<f64>,f64)>>,
    width: u64,
    height: u64,
}

#[derive(Clone, Copy)]
pub struct PosBufId(usize);

#[derive(Clone, Copy)]
pub struct IndBufId(usize);

#[derive(Clone, Copy)]
pub struct ColBufId(usize);

impl Rasterizer {
    pub fn new(w: u64, h: u64) -> Self {
        let mut r = Rasterizer::default();
        r.width = w;
        r.height = h;
        r.frame_buf.resize((w * h) as usize, Vector3::zeros());
        r.depth_buf.resize((w * h) as usize, 0.0);
        //r.frame_sample.resize((w * h) as usize, Vec::new());
        r.texture = None;
        r
    }

    // fn get_index(height: u64, width: u64, x: usize, y: usize) -> usize {
    //     ((height - 1 - y as u64) * width + x as u64) as usize
    // }
    fn get_index(&self, x: usize, y: usize) -> usize {
        ((self.height - 1 - y as u64) * self.width + x as u64) as usize
    }

    fn set_depth(&mut self, point: &Vector3<f64>, depth: f64) -> bool {
        let ind = (self.height as f64 - 1.0 - point.y) * self.width as f64 + point.x;
        if depth>self.depth_buf[ind as usize]{
            self.depth_buf[ind as usize] = depth;
            true
        } else {
            false
        }
    }

    // fn set_pixel(height: u64, width: u64, frame_buf: &mut Vec<Vector3<f64>>, point: &Vector3<f64>, color: &Vector3<f64>) {
    //     let ind = (height as f64 - 1.0 - point.y) * width as f64 + point.x;
    //     frame_buf[ind as usize] = *color;
    // }
    fn set_pixel(&mut self, point: &Vector3<f64>, color: &Vector3<f64>) {
        let ind = (self.height as f64 - 1.0 - point.y) * self.width as f64 + point.x;
        self.frame_buf[ind as usize] = *color;
    }

    pub fn clear(&mut self, buff: Buffer) {
        match buff {
            Buffer::Color => {
                self.frame_buf.fill(Vector3::new(0.0, 0.0, 0.0));
                //self.frame_sample.fill(Vec::new());
            }
            Buffer::Depth => {
                self.depth_buf.fill(f64::NEG_INFINITY);
            }
            Buffer::Both => {
                self.frame_buf.fill(Vector3::new(0.0, 0.0, 0.0));
                self.depth_buf.fill(f64::NEG_INFINITY);
                //self.frame_sample.fill(Vec::new());
            }
        }
    }

    pub fn set_model(&mut self, model: Matrix4<f64>) {
        self.model = model;
    }

    pub fn set_view(&mut self, view: Matrix4<f64>) {
        self.view = view;
    }

    pub fn set_projection(&mut self, projection: Matrix4<f64>) {
        self.projection = projection;
    }

    pub fn set_texture(&mut self, tex: Texture) { 
        self.texture = Some(tex); 
    }

    pub fn set_vertex_shader(&mut self, vert_shader: fn(&VertexShaderPayload) -> Vector3<f64>) {
        self.vert_shader = Some(vert_shader);
    }
    
    pub fn set_fragment_shader(&mut self, frag_shader: fn(&FragmentShaderPayload) -> Vector3<f64>) {
        self.fragment_shader = Some(frag_shader);
    }

    pub fn draw(&mut self, triangles: &Vec<Triangle>) {
        let mvp = self.projection * self.view * self.model;

        // 遍历每个小三角形
        for triangle in triangles { 
            self.rasterize_triangle(&triangle, mvp); 
        }
    }

    pub fn rasterize_triangle(&mut self, triangle: &Triangle, mvp: Matrix4<f64>) {
        /*  Implement your code here  */
        let (t, view_space_pos)=Self::get_new_tri(&triangle,self.view,self.model,mvp,(self.width,self.height));
        let xmin=t.v[0][0].min(t.v[1][0]).min(t.v[2][0]) as usize;
        let xmax=(t.v[0][0].max(t.v[1][0]).max(t.v[2][0])+1.0) as usize;
        let ymin=t.v[0][1].min(t.v[1][1]).min(t.v[2][1]) as usize;
        let ymax=(t.v[0][1].max(t.v[1][1]).max(t.v[2][1])+1.0) as usize;
        for x in xmin..=xmax {
            for y in ymin..=ymax {
                if inside_triangle(x as f64 + 0.5, y as f64 + 0.5,&t.v) {
                    let d=Self::depth(x as f64,y as f64,&t);
                    if self.set_depth(&Vector3::new(x as f64,y as f64 ,0.0), d) {
                        let (a,b,c)=compute_barycentric2d(x as f64, y as f64, &t.v);
                        let view=Self::interpolate_Vec3(a,b,c,view_space_pos[0],view_space_pos[1],view_space_pos[2],1.0);
                        let n=Self::interpolate_Vec3(a,b,c,t.normal[0],t.normal[1],t.normal[2],1.0);
                        let col=Self::interpolate_Vec3(a,b,c,t.color[0],t.color[1],t.color[2],1.0);
                        let tc=Self::interpolate_Vec2(a,b,c,t.tex_coords[0],t.tex_coords[1],t.tex_coords[2],1.0);
                        let mut r=None;
                        if self.texture.is_some() {
                            r=Some(Rc::new(self.texture.as_ref().unwrap()))
                        }
                        //let final_color=self.fragment_shader.unwrap()(&FragmentShaderPayload::new(&col,&n,&tc,r,&view));
                        let final_color=displacement_fragment_shader(&FragmentShaderPayload::new(&col,&n,&tc,r,&view));
                        self.set_pixel(&Vector3::new(x as f64,y as f64 ,0.0), &final_color);
                    }
                }
            }
        }
    }

    pub fn depth(x: f64,y:f64,t: &Triangle) -> f64 {
        let (a,b,c)=compute_barycentric2d(x, y, &t.v);
        a*t.v[0][2]+b*t.v[1][2]+c*t.v[2][2]
    }
    fn interpolate_Vec3(a: f64, b: f64, c: f64, vert1: Vector3<f64>, vert2: Vector3<f64>, vert3: Vector3<f64>, weight: f64) -> Vector3<f64> {
        (a * vert1 + b * vert2 + c * vert3) / weight
    }
    fn interpolate_Vec2(a: f64, b: f64, c: f64, vert1: Vector2<f64>, vert2: Vector2<f64>, vert3: Vector2<f64>, weight: f64) -> Vector2<f64> {
        (a * vert1 + b * vert2 + c * vert3) / weight
    }

    fn get_new_tri(t: &Triangle, view: Matrix4<f64>, model: Matrix4<f64>, mvp: Matrix4<f64>,
                    (width, height): (u64, u64)) -> (Triangle, Vec<Vector3<f64>>) {
        let f1 = (50.0 - 0.1) / 2.0; // zfar和znear距离的一半
        let f2 = (50.0 + 0.1) / 2.0; // zfar和znear的中心z坐标
        let mut new_tri = (*t).clone();
        let mm: Vec<Vector4<f64>> = (0..3).map(|i| view * model * t.v[i]).collect();
        let view_space_pos: Vec<Vector3<f64>> = mm.iter().map(|v| v.xyz()).collect();
        let mut v: Vec<Vector4<f64>> = (0..3).map(|i| mvp * t.v[i]).collect();

        // 换算齐次坐标
        for vec in v.iter_mut() {
            vec.x /= vec.w;
            vec.y /= vec.w;
            vec.z /= vec.w;
        }
        let inv_trans = (view * model).try_inverse().unwrap().transpose();
        let n: Vec<Vector4<f64>> = (0..3).map(|i| inv_trans * to_vec4(t.normal[i], Some(0.0))).collect();

        // 视口变换得到顶点在屏幕上的坐标, 即screen space
        for vert in v.iter_mut() {
            vert.x = 0.5 * width as f64 * (vert.x + 1.0);
            vert.y = 0.5 * height as f64 * (vert.y + 1.0);
            vert.z = vert.z * f1 + f2;
        }
        for i in 0..3 {
            new_tri.set_vertex(i, v[i]);
        }
        for i in 0..3 {
            new_tri.set_normal(i, n[i].xyz());
        }

        new_tri.set_color(0, 148.0, 121.0, 92.0);
        new_tri.set_color(1, 148.0, 121.0, 92.0);
        new_tri.set_color(2, 148.0, 121.0, 92.0);

        (new_tri, view_space_pos)
    }

    pub fn frame_buffer(&self) -> &Vec<Vector3<f64>> {
        &self.frame_buf
    }

}

fn to_vec4(v3: Vector3<f64>, w: Option<f64>) -> Vector4<f64> {
    Vector4::new(v3.x, v3.y, v3.z, w.unwrap_or(1.0))
}

fn inside_triangle(x: f64, y: f64, v: &[Vector4<f64>; 3]) -> bool {
    let v = [
        Vector3::new(v[0].x, v[0].y, 1.0),
        Vector3::new(v[1].x, v[1].y, 1.0),
        Vector3::new(v[2].x, v[2].y, 1.0), ];

    let f0 = v[1].cross(&v[0]);
    let f1 = v[2].cross(&v[1]);
    let f2 = v[0].cross(&v[2]);
    let p = Vector3::new(x, y, 1.0);
    if (p.dot(&f0) * f0.dot(&v[2]) > 0.0) &&
        (p.dot(&f1) * f1.dot(&v[0]) > 0.0) &&
        (p.dot(&f2) * f2.dot(&v[1]) > 0.0) {
        true
    } else {
        false
    }
}

fn compute_barycentric2d(x: f64, y: f64, v: &[Vector4<f64>; 3]) -> (f64, f64, f64) {
    let c1 = (x * (v[1].y - v[2].y) + (v[2].x - v[1].x) * y + v[1].x * v[2].y - v[2].x * v[1].y) / (v[0].x * (v[1].y - v[2].y) + (v[2].x - v[1].x) * v[0].y + v[1].x * v[2].y - v[2].x * v[1].y);
    let c2 = (x * (v[2].y - v[0].y) + (v[0].x - v[2].x) * y + v[2].x * v[0].y - v[0].x * v[2].y) / (v[1].x * (v[2].y - v[0].y) + (v[0].x - v[2].x) * v[1].y + v[2].x * v[0].y - v[0].x * v[2].y);
    let c3 = (x * (v[0].y - v[1].y) + (v[1].x - v[0].x) * y + v[0].x * v[1].y - v[1].x * v[0].y) / (v[2].x * (v[0].y - v[1].y) + (v[1].x - v[0].x) * v[2].y + v[0].x * v[1].y - v[1].x * v[0].y);
    (c1, c2, c3)
}