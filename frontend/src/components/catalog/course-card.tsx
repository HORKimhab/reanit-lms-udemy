import Link from "next/link";
import { formatDuration, formatPrice, LEVEL_LABELS, type CourseCard } from "@/lib/api";

export function CourseCardView({ course }: { course: CourseCard }) {
  return (
    <Link href={`/course/${course.slug}`} className="group block">
      <div className="aspect-video overflow-hidden rounded-lg border bg-muted">
        {course.thumbnail_url && (
          // eslint-disable-next-line @next/next/no-img-element
          <img
            src={course.thumbnail_url}
            alt=""
            className="size-full object-cover transition group-hover:scale-105"
          />
        )}
      </div>
      <h3 className="mt-2 line-clamp-2 font-semibold leading-snug group-hover:underline">{course.title}</h3>
      <p className="text-xs text-muted-foreground">{course.instructor_name}</p>
      <p className="text-xs text-muted-foreground">
        {course.lecture_count} lectures · {formatDuration(course.duration_sec)} · {LEVEL_LABELS[course.level]}
      </p>
      <p className="mt-1 font-bold">{formatPrice(course.price_minor, course.currency)}</p>
    </Link>
  );
}

export function CourseGrid({ courses }: { courses: CourseCard[] }) {
  return (
    <div className="grid grid-cols-1 gap-6 sm:grid-cols-2 lg:grid-cols-4">
      {courses.map((c) => (
        <CourseCardView key={c.id} course={c} />
      ))}
    </div>
  );
}
